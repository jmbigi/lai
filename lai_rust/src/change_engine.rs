//! Change Engine transaccional (v4.0, transversal 4.2).
//!
//! Flujo obligatorio para toda modificación de archivos:
//! `validar estado -> diff -> checkpoint -> aplicar -> verificar integridad`.
//!
//! Cada cambio queda registrado en un journal (`.lai/transactions/<id>.json`)
//! con el contenido original y los hashes SHA-256 (original e intención).
//! Si la verificación post-escritura falla (el archivo en disco no coincide
//! con el hash pretendido), se revierte automáticamente: el contenido original
//! se restaura (o el archivo nuevo se elimina). Un worktree revertido con un
//! proceso hijo escribiendo no es un rollback: la verificación es por hash
//! sobre el estado final en disco.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Estado de una transacción de cambio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionStatus {
    Staged,
    Applied,
    Verified,
    RolledBack,
    Failed,
}

impl TransactionStatus {
    pub fn label(&self) -> &'static str {
        match self {
            TransactionStatus::Staged => "STAGED",
            TransactionStatus::Applied => "APPLIED",
            TransactionStatus::Verified => "VERIFIED",
            TransactionStatus::RolledBack => "ROLLED_BACK",
            TransactionStatus::Failed => "FAILED",
        }
    }
}

/// Transacción de cambio de un archivo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeTransaction {
    pub id: String,
    pub path: String,
    /// Contenido original (None = archivo nuevo).
    pub original_content: Option<String>,
    pub original_hash: Option<String>,
    /// Hash SHA-256 del contenido pretendido.
    pub intended_hash: String,
    pub status: TransactionStatus,
}

/// Motor de cambios transaccionales.
pub struct ChangeEngine {
    workspace_root: PathBuf,
    journal_dir: PathBuf,
}

/// Hash SHA-256 en hexadecimal de un contenido (delega en integrity.rs).
pub fn sha256_hex(content: &str) -> String {
    crate::integrity::sha256_hex(content)
}

impl ChangeEngine {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            journal_dir: workspace_root.join(".lai").join("transactions"),
        }
    }

    /// Resuelve la ruta absoluta del archivo dentro del workspace.
    fn resolve(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.workspace_root.join(p)
        }
    }

    fn journal_path(&self, id: &str) -> PathBuf {
        self.journal_dir.join(format!("{}.json", id))
    }

    fn save_journal(&self, tx: &ChangeTransaction) -> std::io::Result<()> {
        fs::create_dir_all(&self.journal_dir)?;
        fs::write(
            self.journal_path(&tx.id),
            serde_json::to_string_pretty(tx)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        )
    }

    /// Paso 1-2: valida el estado actual y registra el diff/checkpoint
    /// (contenido original + hashes) antes de tocar el disco.
    pub fn stage(&self, path: &str, new_content: &str) -> std::io::Result<ChangeTransaction> {
        let abs = self.resolve(path);
        let original_content = if abs.exists() {
            Some(fs::read_to_string(&abs)?)
        } else {
            None
        };
        let tx = ChangeTransaction {
            id: Uuid::new_v4().to_string(),
            path: path.to_string(),
            original_hash: original_content.as_deref().map(sha256_hex),
            intended_hash: sha256_hex(new_content),
            original_content,
            status: TransactionStatus::Staged,
        };
        self.save_journal(&tx)?;
        Ok(tx)
    }

    /// Paso 3: aplica la escritura de forma atómica (tmp + rename).
    pub fn apply(&self, tx: &mut ChangeTransaction, new_content: &str) -> std::io::Result<()> {
        let abs = self.resolve(&tx.path);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = abs.with_extension(format!(
            "lai-tmp-{}",
            Uuid::new_v4().to_string().split('-').next().unwrap_or("x")
        ));
        fs::write(&tmp, new_content)?;
        fs::rename(&tmp, &abs)?;
        tx.status = TransactionStatus::Applied;
        self.save_journal(tx)?;
        Ok(())
    }

    /// Paso 4: verifica integridad por hash sobre el estado final en disco.
    /// Devuelve Ok(true) si coincide; Ok(false) si no (el llamador debe
    /// revertir). Un fallo de lectura se trata como no verificado.
    pub fn verify(&self, tx: &mut ChangeTransaction) -> std::io::Result<bool> {
        let abs = self.resolve(&tx.path);
        let on_disk = fs::read_to_string(&abs).unwrap_or_default();
        if sha256_hex(&on_disk) == tx.intended_hash {
            tx.status = TransactionStatus::Verified;
            self.save_journal(tx)?;
            Ok(true)
        } else {
            tx.status = TransactionStatus::Failed;
            self.save_journal(tx)?;
            Ok(false)
        }
    }

    /// Revierte la transacción: restaura el contenido original o elimina el
    /// archivo nuevo. La reversión se verifica por hash.
    pub fn rollback(&self, tx: &mut ChangeTransaction) -> std::io::Result<bool> {
        let abs = self.resolve(&tx.path);
        match &tx.original_content {
            Some(original) => {
                fs::write(&abs, original)?;
                tx.status = TransactionStatus::RolledBack;
            }
            None => {
                if abs.exists() {
                    fs::remove_file(&abs)?;
                }
                tx.status = TransactionStatus::RolledBack;
            }
        }
        self.save_journal(tx)?;
        // Verificación post-rollback: el estado en disco debe coincidir con
        // el original (o el archivo debe haber desaparecido).
        let now = fs::read_to_string(&abs).unwrap_or_default();
        Ok(match &tx.original_hash {
            Some(h) => sha256_hex(&now) == *h,
            None => !abs.exists(),
        })
    }

    /// Ejecuta el ciclo completo: stage -> apply -> verify, con rollback
    /// automático si la verificación falla. Devuelve Ok(true) si el cambio
    /// quedó verificado en disco.
    pub fn commit(&self, path: &str, new_content: &str) -> std::io::Result<bool> {
        let mut tx = self.stage(path, new_content)?;
        self.apply(&mut tx, new_content)?;
        if self.verify(&mut tx)? {
            Ok(true)
        } else {
            self.rollback(&mut tx)?;
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_ws() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lai-ce-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_commit_new_file_verified() {
        let ws = temp_ws();
        let ce = ChangeEngine::new(&ws);
        assert!(ce.commit("nuevo.txt", "contenido").unwrap());
        assert_eq!(fs::read_to_string(ws.join("nuevo.txt")).unwrap(), "contenido");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn test_commit_existing_file_replaces() {
        let ws = temp_ws();
        fs::write(ws.join("a.txt"), "viejo").unwrap();
        let ce = ChangeEngine::new(&ws);
        assert!(ce.commit("a.txt", "nuevo").unwrap());
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "nuevo");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn test_verify_detects_tampering_and_rollback_restores() {
        let ws = temp_ws();
        fs::write(ws.join("a.txt"), "original").unwrap();
        let ce = ChangeEngine::new(&ws);

        // Transacción manual: stage + apply, pero se manipula el disco antes
        // de verificar (simula un proceso externo escribiendo).
        let mut tx = ce.stage("a.txt", "pretendido").unwrap();
        ce.apply(&mut tx, "pretendido").unwrap();
        fs::write(ws.join("a.txt"), "manipulado por terceros").unwrap();

        assert!(!ce.verify(&mut tx).unwrap());
        assert_eq!(tx.status, TransactionStatus::Failed);

        // Rollback: restaura el original y lo verifica por hash.
        assert!(ce.rollback(&mut tx).unwrap());
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "original");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn test_rollback_new_file_removes_it() {
        let ws = temp_ws();
        let ce = ChangeEngine::new(&ws);
        let mut tx = ce.stage("nuevo.txt", "contenido").unwrap();
        ce.apply(&mut tx, "contenido").unwrap();
        fs::write(ws.join("nuevo.txt"), "manipulado").unwrap();
        assert!(!ce.verify(&mut tx).unwrap());
        assert!(ce.rollback(&mut tx).unwrap());
        assert!(!ws.join("nuevo.txt").exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn test_journal_persisted() {
        let ws = temp_ws();
        let ce = ChangeEngine::new(&ws);
        let tx = ce.stage("a.txt", "x").unwrap();
        let journal = ws.join(".lai").join("transactions").join(format!("{}.json", tx.id));
        assert!(journal.exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn test_sha256_hex_known_vector() {
        assert_eq!(sha256_hex(""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}