"""RAG semántico local 100% open source para lai.

Usa sentence-transformers para embeddings y numpy para similitud de coseno.
Almacenamiento 100% local en JSON + numpy .npy. Sin cloud, sin API keys,
sin cuentas externas. Ideal para indexar código y documentación de proyectos.

Instalación de dependencias:
    pip install sentence-transformers numpy watchdog
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import re
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import numpy as np

try:
    from sentence_transformers import SentenceTransformer

    _HAS_ST = True
except ImportError:  # pragma: no cover
    _HAS_ST = False

try:
    from watchdog.observers import Observer
    from watchdog.events import FileSystemEventHandler

    _HAS_WATCHDOG = True
except ImportError:  # pragma: no cover
    _HAS_WATCHDOG = False


STORAGE_DIR = Path(os.environ.get("LAI_STORAGE_DIR", Path.home() / ".local-ai"))
COLLECTIONS_FILE = STORAGE_DIR / "collections.json"
EMBEDDINGS_FILE = STORAGE_DIR / "embeddings.npy"
METADATA_FILE = STORAGE_DIR / "metadata.json"

_DEFAULT_MODEL = "all-MiniLM-L6-v2"
_MSG_INSTALL_ST = "install sentence-transformers"


class LocalRAG:
    """Motor RAG semántico local con embeddings y búsqueda por similitud de coseno.

    Características:
    - Embeddings con sentence-transformers (modelo local descargado una vez)
    - Búsqueda semántica sobre código y documentación
    - Persistencia en disco (JSON + numpy)
    - Multi-colección (aislamiento por proyecto)
    - 100% local, sin cloud, sin API keys
    """

    def __init__(self, model_name: str = _DEFAULT_MODEL, storage_dir: Path | None = None):
        if not _HAS_ST:
            raise ImportError(_MSG_INSTALL_ST)
        self.storage_dir = storage_dir or STORAGE_DIR
        self.storage_dir.mkdir(parents=True, exist_ok=True)
        self.model_name = model_name
        self._model: SentenceTransformer | None = None
        self._collections: dict[str, Any] = {}
        self._embeddings: np.ndarray | None = None
        self._metadata: list[dict[str, Any]] = []
        self._load()

    @property
    def model(self) -> SentenceTransformer:
        if self._model is None:
            self._model = SentenceTransformer(self.model_name)
        return self._model

    def _load(self) -> None:
        try:
            self._collections = json.loads(
                (self.storage_dir / "collections.json").read_text(encoding="utf-8")
            )
        except (FileNotFoundError, json.JSONDecodeError):
            self._collections = {}

        try:
            self._embeddings = np.load(self.storage_dir / "embeddings.npy")
        except (FileNotFoundError, Exception):
            self._embeddings = None

        try:
            self._metadata = json.loads(
                (self.storage_dir / "metadata.json").read_text(encoding="utf-8")
            )
        except (FileNotFoundError, json.JSONDecodeError):
            self._metadata = []

    def _save(self) -> None:
        (self.storage_dir / "collections.json").write_text(
            json.dumps(self._collections, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        if self._embeddings is not None:
            np.save(self.storage_dir / "embeddings.npy", self._embeddings)
        (self.storage_dir / "metadata.json").write_text(
            json.dumps(self._metadata, ensure_ascii=False, indent=2), encoding="utf-8"
        )

    def create_collection(self, name: str) -> None:
        if name in self._collections:
            return
        self._collections[name] = {
            "created_at": datetime.now(timezone.utc).isoformat(),
            "document_count": 0,
        }
        self._save()

    def list_collections(self) -> list[str]:
        return list(self._collections.keys())

    def delete_collection(self, name: str) -> bool:
        if name not in self._collections:
            return False
        del self._collections[name]
        keep_indices = [i for i, m in enumerate(self._metadata) if m.get("collection") != name]
        if keep_indices:
            self._embeddings = (
                self._embeddings[keep_indices] if self._embeddings is not None else None
            )
            self._metadata = [self._metadata[i] for i in keep_indices]
        else:
            self._embeddings = None
            self._metadata = []
        self._save()
        return True

    def add_documents(
        self,
        collection: str,
        documents: list[str],
        metadatas: list[dict[str, Any]] | None = None,
        ids: list[str] | None = None,
    ) -> list[str]:  # noqa: PLR0913
        if collection not in self._collections:
            self.create_collection(collection)

        if metadatas is None:
            metadatas = [{} for _ in documents]
        if ids is None:
            ids = [
                f"{collection}_{i}_{datetime.now(timezone.utc).timestamp()}"
                for i in range(len(documents))
            ]

        embeddings = self.model.encode(documents, show_progress_bar=False)
        embeddings = np.array(embeddings, dtype=np.float32)

        if self._embeddings is None:
            self._embeddings = embeddings
        else:
            self._embeddings = np.vstack([self._embeddings, embeddings])

        start_idx = len(self._metadata)
        for i, (doc_id, meta) in enumerate(zip(ids, metadatas, strict=True)):
            self._metadata.append(
                {
                    "id": doc_id,
                    "collection": collection,
                    "text": documents[i],
                    "metadata": meta,
                    "index": start_idx + i,
                    "added_at": datetime.now(timezone.utc).isoformat(),
                }
            )

        self._collections[collection]["document_count"] = self._collections[collection].get(
            "document_count", 0
        ) + len(documents)
        self._save()
        return ids

    def add_file(self, collection: str, file_path: str | Path) -> str | None:
        path = Path(file_path)
        if not path.exists():
            return None
        text = path.read_text(encoding="utf-8", errors="ignore")
        if not text.strip():
            return None
        doc_id = f"{collection}:{path}"
        self.add_documents(
            collection,
            [text],
            metadatas=[{"file": str(path), "type": path.suffix}],
            ids=[doc_id],
        )
        return doc_id

    def add_directory(
        self,
        collection: str,
        directory: str | Path,
        extensions: tuple[str, ...] = (".py", ".md", ".txt", ".json", ".yaml", ".yml"),
    ) -> int:
        dir_path = Path(directory)
        count = 0
        for ext in extensions:
            for f in dir_path.rglob(f"*{ext}"):
                if self.add_file(collection, f):
                    count += 1
        return count

    # ------------------------------------------------------------------
    # Indexación incremental
    # ------------------------------------------------------------------

    def _file_hash(self, path: Path) -> str:
        """Calcula SHA-256 del contenido de un archivo."""
        h = hashlib.sha256()
        try:
            with open(path, "rb") as f:
                for chunk in iter(lambda: f.read(8192), b""):
                    h.update(chunk)
        except OSError:
            return ""
        return h.hexdigest()

    def _doc_index_by_id(self, doc_id: str) -> int | None:
        for i, m in enumerate(self._metadata):
            if m.get("id") == doc_id:
                return i
        return None

    def _doc_index_by_file(self, file_path: str) -> int | None:
        for i, m in enumerate(self._metadata):
            if m.get("metadata", {}).get("file") == file_path:
                return i
        return None

    def add_or_update_document(
        self,
        collection: str,
        document: str,
        metadata: dict[str, Any] | None = None,
        doc_id: str | None = None,
    ) -> str | None:
        """Agrega un documento nuevo o actualiza uno existente por doc_id.

        Si el doc_id ya existe, reemplaza su embedding y metadatos en su lugar
        en lugar de duplicar el vector. Devuelve el doc_id usado.
        """
        if collection not in self._collections:
            self.create_collection(collection)

        if metadata is None:
            metadata = {}
        if doc_id is None:
            doc_id = f"{collection}_{datetime.now(timezone.utc).timestamp()}"

        existing_idx = self._doc_index_by_id(doc_id)

        embedding = self.model.encode([document], show_progress_bar=False)[0]
        embedding = np.array(embedding, dtype=np.float32).reshape(1, -1)

        now = datetime.now(timezone.utc).isoformat()
        meta_entry = {
            "id": doc_id,
            "collection": collection,
            "text": document,
            "metadata": metadata,
            "updated_at": now,
        }

        if existing_idx is not None:
            # Reemplazar embedding en su lugar
            old_index = self._metadata[existing_idx].get("index", existing_idx)
            self._metadata[existing_idx] = meta_entry
            self._metadata[existing_idx]["index"] = old_index
            self._metadata[existing_idx]["added_at"] = self._metadata[existing_idx].get(
                "added_at", now
            )

            if self._embeddings is not None:
                self._embeddings[old_index] = embedding[0]
            else:
                self._embeddings = embedding
        else:
            if self._embeddings is None:
                self._embeddings = embedding
                new_index = 0
            else:
                self._embeddings = np.vstack([self._embeddings, embedding])
                new_index = len(self._metadata)

            meta_entry["index"] = new_index
            meta_entry["added_at"] = now
            self._metadata.append(meta_entry)

        self._collections[collection]["document_count"] = len(
            [m for m in self._metadata if m.get("collection") == collection]
        )
        self._save()
        return doc_id

    def add_or_update_file(
        self,
        collection: str,
        file_path: str | Path,
    ) -> str | None:
        """Agrega o actualiza un archivo en la colección.

        Si el archivo ya existe y su hash no cambió, no hace nada.
        Devuelve el doc_id si se agregó/actualizó, None si no hubo cambios.
        """
        path = Path(file_path)
        if not path.exists():
            return None
        text = path.read_text(encoding="utf-8", errors="ignore")
        if not text.strip():
            return None

        file_hash = self._file_hash(path)
        file_str = str(path)
        existing_idx = self._doc_index_by_file(file_str)

        if existing_idx is not None:
            old_meta = self._metadata[existing_idx]
            old_hash = old_meta.get("metadata", {}).get("hash", "")
            if old_hash == file_hash:
                return None  # Sin cambios
            doc_id = old_meta["id"]
        else:
            doc_id = f"{collection}:{path}"

        metadata = {
            "file": file_str,
            "type": path.suffix,
            "hash": file_hash,
        }
        return self.add_or_update_document(
            collection,
            text,
            metadata=metadata,
            doc_id=doc_id,
        )

    def index_directory_incremental(
        self,
        collection: str,
        directory: str | Path,
        extensions: tuple[str, ...] = (".py", ".md", ".txt", ".json", ".yaml", ".yml"),
    ) -> dict[str, int]:
        """Indexa solo archivos nuevos o modificados desde la última indexación.

        Devuelve un diccionario con contadores: added, updated, skipped, total.
        """
        dir_path = Path(directory)
        stats = {"added": 0, "updated": 0, "skipped": 0, "total": 0}

        for ext in extensions:
            for f in dir_path.rglob(f"*{ext}"):
                stats["total"] += 1
                file_str = str(f)
                existing_idx = self._doc_index_by_file(file_str)

                if existing_idx is not None:
                    old_meta = self._metadata[existing_idx]
                    old_hash = old_meta.get("metadata", {}).get("hash", "")
                    new_hash = self._file_hash(f)
                    if old_hash == new_hash:
                        stats["skipped"] += 1
                        continue

                result = self.add_or_update_file(collection, f)
                if result is not None:
                    if existing_idx is not None:
                        stats["updated"] += 1
                    else:
                        stats["added"] += 1

        return stats

    # ------------------------------------------------------------------
    # Chunking inteligente de código
    # ------------------------------------------------------------------

    @staticmethod
    def _chunk_code(
        text: str,
        max_chunk_size: int = 2000,
        overlap: int = 200,
    ) -> list[str]:
        """Divide código en bloques semánticos (funciones, clases, bloques).

        Intenta respetar límites de funciones y clases para no romper
        el contexto. Si un bloque es muy grande, lo divide por líneas.
        """
        if not text.strip():
            return []

        lines = text.split("\n")
        chunks: list[str] = []
        current_chunk: list[str] = []
        current_size = 0
        brace_depth = 0

        for i, line in enumerate(lines):
            stripped = line.strip()

            is_block_start = bool(
                re.match(
                    r"^(def |class |async def |func |fn |public |private |protected )",
                    stripped,
                )
            ) or stripped.endswith("{")

            brace_depth += line.count("{") - line.count("}")

            if is_block_start and current_chunk and current_size > 0:
                chunks.append("\n".join(current_chunk))
                current_chunk = []
                current_size = 0

            current_chunk.append(line)
            current_size += len(line) + 1

            if current_size >= max_chunk_size:
                chunks.append("\n".join(current_chunk))
                current_chunk = []
                current_size = 0

            if brace_depth == 0 and current_chunk and stripped.endswith("}"):
                chunks.append("\n".join(current_chunk))
                current_chunk = []
                current_size = 0

        if current_chunk:
            chunks.append("\n".join(current_chunk))

        if not chunks:
            for i in range(0, len(lines), max_chunk_size // 80):
                chunk_lines = lines[i : i + max_chunk_size // 80]
                if chunk_lines:
                    chunks.append("\n".join(chunk_lines))

        return [c for c in chunks if c.strip()]

    def add_file_chunked(
        self,
        collection: str,
        file_path: str | Path,
        max_chunk_size: int = 2000,
    ) -> list[str] | None:
        path = Path(file_path)
        if not path.exists():
            return None
        text = path.read_text(encoding="utf-8", errors="ignore")
        if not text.strip():
            return None

        chunks = self._chunk_code(text, max_chunk_size=max_chunk_size)
        if not chunks:
            return None

        doc_ids: list[str] = []
        base_id = f"{collection}:{path}"
        for i, chunk in enumerate(chunks):
            doc_id = f"{base_id}#chunk{i}"
            self.add_or_update_document(
                collection,
                chunk,
                metadata={
                    "file": str(path),
                    "type": path.suffix,
                    "chunk": i,
                    "total_chunks": len(chunks),
                },
                doc_id=doc_id,
            )
            doc_ids.append(doc_id)

        return doc_ids

    def add_directory_chunked(
        self,
        collection: str,
        directory: str | Path,
        extensions: tuple[str, ...] = (".py", ".md", ".txt", ".json", ".yaml", ".yml"),
        max_chunk_size: int = 2000,
    ) -> dict[str, int]:
        dir_path = Path(directory)
        stats = {"files": 0, "chunks_added": 0, "total_chunks": 0}

        for ext in extensions:
            for f in dir_path.rglob(f"*{ext}"):
                result = self.add_file_chunked(collection, f, max_chunk_size=max_chunk_size)
                if result:
                    stats["files"] += 1
                    stats["chunks_added"] += len(result)
                    stats["total_chunks"] += len(result)

        return stats

    # ------------------------------------------------------------------
    # Búsqueda híbrida: vector + keyword (TF-IDF simple)
    # ------------------------------------------------------------------

    def _tfidf_score(
        self,
        query_terms: list[str],
        doc_text: str,
    ) -> float:
        if not query_terms or not doc_text:
            return 0.0

        doc_terms = doc_text.lower().split()
        doc_len = len(doc_terms)
        if doc_len == 0:
            return 0.0

        doc_counter = Counter(doc_terms)
        total_docs = max(len(self._metadata), 1)

        score = 0.0
        for term in query_terms:
            term = term.lower()
            tf = doc_counter.get(term, 0) / doc_len
            docs_with_term = sum(
                1 for m in self._metadata if term in m.get("text", "").lower().split()
            )
            idf = math.log(total_docs / max(docs_with_term, 1)) + 1.0
            score += tf * idf

        return score

    def search_hybrid(
        self,
        query: str,
        collection: str | None = None,
        top_k: int = 5,
        threshold: float = 0.3,
        vector_weight: float = 0.7,
        keyword_weight: float = 0.3,
    ) -> list[dict[str, Any]]:
        if self._embeddings is None or len(self._metadata) == 0:
            return []

        vector_results = self.search(
            query=query,
            collection=collection,
            top_k=top_k * 3,
            threshold=0.1,
        )

        query_terms = [t for t in re.split(r"\s+", query.lower()) if t]

        hybrid_scores: dict[str, dict[str, Any]] = {}
        max_vector = max((r["score"] for r in vector_results), default=0.0) or 1.0

        for r in vector_results:
            doc_id = r["id"]
            text = r.get("text", "")
            keyword_score = self._tfidf_score(query_terms, text)

            norm_vector = r["score"] / max_vector
            combined = (vector_weight * norm_vector) + (keyword_weight * keyword_score)

            hybrid_scores[doc_id] = {
                "id": r["id"],
                "collection": r["collection"],
                "text": text,
                "metadata": r.get("metadata", {}),
                "vector_score": r["score"],
                "keyword_score": keyword_score,
                "score": combined,
            }

        if collection:
            hybrid_scores = {
                k: v for k, v in hybrid_scores.items() if v["collection"] == collection
            }

        sorted_results = sorted(hybrid_scores.values(), key=lambda x: x["score"], reverse=True)

        final_results: list[dict[str, Any]] = []
        for r in sorted_results:
            if r["score"] < threshold:
                continue
            final_results.append(
                {
                    "id": r["id"],
                    "collection": r["collection"],
                    "text": r["text"],
                    "metadata": r["metadata"],
                    "score": r["score"],
                    "vector_score": r["vector_score"],
                    "keyword_score": r["keyword_score"],
                }
            )
            if len(final_results) >= top_k:
                break

        return final_results

    def search(
        self,
        query: str,
        collection: str | None = None,
        top_k: int = 5,
        threshold: float = 0.3,
    ) -> list[dict[str, Any]]:
        if self._embeddings is None or len(self._metadata) == 0:
            return []

        query_emb = self.model.encode([query], show_progress_bar=False)[0]
        query_emb = np.array(query_emb, dtype=np.float32).reshape(1, -1)

        emb = self._embeddings
        norms = np.linalg.norm(emb, axis=1, keepdims=True)
        qnorm = np.linalg.norm(query_emb, axis=1, keepdims=True)
        with np.errstate(divide="ignore", invalid="ignore"):
            sims = (emb @ query_emb.T).flatten() / (norms.flatten() * qnorm.flatten())

        results: list[dict[str, Any]] = []
        for idx in np.argsort(sims)[::-1]:
            if sims[idx] < threshold:
                continue
            meta = self._metadata[int(idx)]
            if collection and meta.get("collection") != collection:
                continue
            results.append(
                {
                    "id": meta["id"],
                    "collection": meta["collection"],
                    "text": meta["text"],
                    "metadata": meta.get("metadata", {}),
                    "score": float(sims[idx]),
                }
            )
            if len(results) >= top_k:
                break
        return results

    def get_collection_info(self, name: str) -> dict[str, Any] | None:
        return self._collections.get(name)

    def stats(self) -> dict[str, Any]:
        return {
            "collections": len(self._collections),
            "total_documents": len(self._metadata),
            "embedding_dim": int(self._embeddings.shape[1]) if self._embeddings is not None else 0,
            "model": self.model_name,
            "storage_dir": str(self.storage_dir),
        }


def get_rag() -> LocalRAG:
    if not hasattr(get_rag, "_instance"):
        get_rag._instance = LocalRAG()  # type: ignore[attr-defined]
    return get_rag._instance  # type: ignore[return-value]


# ============================================================================
# RAG Watcher - monitor de archivos para indexación incremental automática
# ============================================================================

if _HAS_WATCHDOG:

    class _RAGEventHandler(FileSystemEventHandler):
        """Handler de eventos de watchdog que reindexa archivos modificados."""

        def __init__(self, rag: LocalRAG, collection: str, extensions: tuple[str, ...]):
            self.rag = rag
            self.collection = collection
            self.extensions = extensions
            self._debounce: dict[str, float] = {}

        def _should_process(self, path: str) -> bool:
            """Filtro simple de debounce: ignora eventos muy rápidos del mismo archivo."""
            now = time.time()
            last = self._debounce.get(path, 0)
            if now - last < 1.0:
                return False
            self._debounce[path] = now
            return True

        def on_modified(self, event):
            if event.is_directory:
                return
            path = Path(event.src_path)
            if path.suffix not in self.extensions:
                return
            if not self._should_process(str(path)):
                return
            self.rag.add_or_update_file(self.collection, path)

        def on_created(self, event):
            if event.is_directory:
                return
            path = Path(event.src_path)
            if path.suffix not in self.extensions:
                return
            if not self._should_process(str(path)):
                return
            self.rag.add_or_update_file(self.collection, path)

        def on_deleted(self, event):
            if event.is_directory:
                return
            path = Path(event.src_path)
            if path.suffix not in self.extensions:
                return
            idx = self.rag._doc_index_by_file(str(path))
            if idx is not None:
                doc_id = self.rag._metadata[idx].get("id")
                if doc_id:
                    self.rag.delete_document(doc_id)


if _HAS_WATCHDOG:

    class RAGWatcher:
        """Monitor de directorio para indexación RAG incremental en tiempo real.

        Requiere `watchdog` (pip install watchdog). Si no está disponible,
        se puede usar indexación manual con index_directory_incremental.
        """

        def __init__(
            self,
            rag: LocalRAG,
            collection: str,
            directory: str | Path,
            extensions: tuple[str, ...] = (".py", ".md", ".txt", ".json", ".yaml", ".yml"),
        ):
            if not _HAS_WATCHDOG:
                raise ImportError("watchdog no instalado. Ejecuta: pip install watchdog")
            self.rag = rag
            self.collection = collection
            self.directory = Path(directory)
            self.extensions = extensions
            self._observer: Observer | None = None

        def start(self) -> None:
            """Inicia el monitoreo del directorio en un hilo separado."""
            if self._observer is not None:
                return
            # Indexación inicial incremental
            self.rag.index_directory_incremental(self.collection, self.directory, self.extensions)
            handler = _RAGEventHandler(self.rag, self.collection, self.extensions)
            self._observer = Observer()
            self._observer.schedule(handler, str(self.directory), recursive=True)
            self._observer.start()

        def stop(self) -> None:
            """Detiene el monitoreo."""
            if self._observer is not None:
                self._observer.stop()
                self._observer.join()
                self._observer = None

        def is_running(self) -> bool:
            return self._observer is not None and self._observer.is_alive()

else:

    class RAGWatcher:  # type: ignore[no-redef]
        """Stubs cuando watchdog no está instalado."""

        def __init__(self, *args, **kwargs):
            raise ImportError("watchdog no instalado. Ejecuta: pip install watchdog")

        def start(self):
            raise ImportError("watchdog no instalado. Ejecuta: pip install watchdog")

        def stop(self):
            pass

        def is_running(self):
            return False


# Agregar método delete_document a LocalRAG (se usa por el watcher)
def _patch_delete_document():
    """Añade delete_document a LocalRAG si no existe."""
    if not hasattr(LocalRAG, "delete_document"):

        def delete_document(self, doc_id: str) -> bool:
            idx = self._doc_index_by_id(doc_id)
            if idx is None:
                return False
            old_index = self._metadata[idx].get("index", idx)
            del self._metadata[idx]
            if self._embeddings is not None:
                keep = [i for i in range(len(self._embeddings)) if i != old_index]
                if keep:
                    self._embeddings = self._embeddings[keep]
                    # Reindexar índices de metadata
                    index_map = {old: new for new, old in enumerate(keep)}
                    for m in self._metadata:
                        old_idx = m.get("index")
                        if old_idx is not None and old_idx in index_map:
                            m["index"] = index_map[old_idx]
                else:
                    self._embeddings = None
            for coll in self._collections.values():
                coll["document_count"] = len(
                    [m for m in self._metadata if m.get("collection") == coll.get("name", "")]
                )
            self._save()
            return True

        LocalRAG.delete_document = delete_document


_patch_delete_document()
