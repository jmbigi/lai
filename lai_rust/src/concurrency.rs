//! Locks de coordinación para concurrencia (v4.0, transversal 4.4).
//!
//! Dos tareas que comparten el mismo índice RepoMap pero operan sobre
//! worktrees distintos **no pueden indexar concurrentemente**: el índice es
//! un recurso compartido. Política de bloqueo:
//!
//! - **Lock de índice** por par `(repo, índice)`: exclusivo para escritura
//!   del índice (build/update del RepoMap).
//! - **Lock de worktree** por par `(repo, worktree)`: para cambios de
//!   archivo dentro de un worktree.
//!
//! Implementación: bloqueos basados en el filesystem (mkdir atómico) para
//! coordinar entre procesos. Si el proceso dueño murió (PID no existente),
//! el lock se considera stale y puede recuperarse.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Error de adquisición de lock.
#[derive(Debug)]
pub enum LockError {
    /// El lock está en posesión de otro proceso vivo.
    Busy { owner_pid: u32, path: PathBuf },
    /// No se pudo crear el directorio de lock.
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Busy { owner_pid, path } => {
                write!(f, "lock ocupado por PID {} ({})", owner_pid, path.display())
            }
            LockError::Io(e) => write!(f, "error de filesystem: {}", e),
        }
    }
}

impl From<std::io::Error> for LockError {
    fn from(e: std::io::Error) -> Self {
        LockError::Io(e)
    }
}

/// Lock adquirido. Se libera automáticamente al salir de scope (Drop).
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceLock {
    pub path: PathBuf,
    pub owner_pid: u32,
    pub kind: String,
}

impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Gestor de locks por workspace.
pub struct LockManager {
    base_dir: PathBuf,
}

/// Comprueba si un PID está vivo (Linux: /proc/<pid>).
fn pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{}", pid)).is_dir()
}

impl LockManager {
    pub fn new(workspace: &Path) -> Self {
        Self {
            base_dir: workspace.join(".lai").join("locks"),
        }
    }

    fn lock_path(&self, kind: &str, key: &str) -> PathBuf {
        // Clave saneada: alfanuméricos y '-'/'_' (-> '-'), máx 60 chars.
        let safe: String = key
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .collect::<String>()
            .chars()
            .take(60)
            .collect();
        self.base_dir.join(format!("{}-{}", kind, safe))
    }

    fn try_create(&self, path: &Path, kind: &str, key: &str) -> Result<WorkspaceLock, LockError> {
        // El directorio base puede no existir aún (primera ejecución).
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::create_dir(path) {
            Ok(()) => {
                let pid = std::process::id();
                let _ = fs::write(path.join("owner"), pid.to_string());
                let _ = fs::write(path.join("kind"), kind);
                Ok(WorkspaceLock { path: path.to_path_buf(), owner_pid: pid, kind: kind.to_string() })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // ¿Dueño vivo? Si no, lock stale: recuperar.
                let owner_pid = fs::read_to_string(path.join("owner"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(0);
                if owner_pid > 0 && pid_alive(owner_pid) && owner_pid != std::process::id() {
                    return Err(LockError::Busy { owner_pid, path: path.to_path_buf() });
                }
                // Stale (o nuestro propio proceso): limpiar y reintentar.
                let _ = fs::remove_dir_all(path);
                self.try_create(path, kind, key)
            }
            Err(e) => Err(LockError::Io(e)),
        }
    }

    fn acquire_with_retry(&self, kind: &str, key: &str, timeout: Duration) -> Result<WorkspaceLock, LockError> {
        let path = self.lock_path(kind, key);
        let start = Instant::now();
        loop {
            match self.try_create(&path, kind, key) {
                Ok(lock) => return Ok(lock),
                Err(LockError::Busy { .. }) if start.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Lock exclusivo de índice por `(repo, índice)`: para build/update del
    /// RepoMap. `repo_key` identifica el repositorio (normalmente su raíz).
    pub fn acquire_index_lock(&self, repo_key: &str, timeout: Duration) -> Result<WorkspaceLock, LockError> {
        self.acquire_with_retry("index", repo_key, timeout)
    }

    /// Lock por `(repo, worktree)`: para cambios de archivo dentro de un
    /// worktree concreto.
    pub fn acquire_worktree_lock(
        &self,
        repo_key: &str,
        worktree: &str,
        timeout: Duration,
    ) -> Result<WorkspaceLock, LockError> {
        self.acquire_with_retry("worktree", &format!("{}::{}", repo_key, worktree), timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_ws() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lai-lock-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_acquire_and_release() {
        let ws = temp_ws();
        let lm = LockManager::new(&ws);
        let lock = lm.acquire_index_lock("repo-a", Duration::from_secs(1)).unwrap();
        assert!(lock.path.is_dir());
        drop(lock);
        // Tras liberar, se puede readquirir.
        let lock2 = lm.acquire_index_lock("repo-a", Duration::from_secs(1)).unwrap();
        drop(lock2);
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn test_second_acquire_busy_while_held() {
        let ws = temp_ws();
        let lm = LockManager::new(&ws);
        // Simular un lock poseído por OTRO proceso vivo (PID 1 = init).
        let held = lm.lock_path("index", "repo-a");
        fs::create_dir_all(&held).unwrap();
        fs::write(held.join("owner"), "1").unwrap();
        let err = lm.acquire_index_lock("repo-a", Duration::from_millis(200));
        assert!(matches!(err, Err(LockError::Busy { .. })));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn test_stale_lock_recovered() {
        let ws = temp_ws();
        let lm = LockManager::new(&ws);
        // Simular lock stale: PID inexistente (999999).
        let stale = lm.lock_path("index", "repo-a");
        fs::create_dir_all(&stale).unwrap();
        fs::write(stale.join("owner"), "999999").unwrap();
        let lock = lm.acquire_index_lock("repo-a", Duration::from_secs(1));
        assert!(lock.is_ok(), "un lock stale debe recuperarse");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn test_index_and_worktree_locks_independent() {
        let ws = temp_ws();
        let lm = LockManager::new(&ws);
        let idx = lm.acquire_index_lock("repo-a", Duration::from_secs(1)).unwrap();
        // Distinto worktree: no bloqueado por el lock de índice.
        let wt = lm.acquire_worktree_lock("repo-a", "wt-1", Duration::from_secs(1));
        assert!(wt.is_ok());
        drop(idx);
        drop(wt);
        let _ = fs::remove_dir_all(&ws);
    }
}