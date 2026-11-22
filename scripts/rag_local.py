"""RAG semántico local 100% open source para lai.

Usa sentence-transformers para embeddings y numpy para similitud de coseno.
Almacenamiento 100% local en JSON + numpy .npy. Sin cloud, sin API keys,
sin cuentas externas. Ideal para indexar código y documentación de proyectos.

Instalación de dependencias:
    pip install sentence-transformers numpy
"""

from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import numpy as np

try:
    from sentence_transformers import SentenceTransformer

    _HAS_ST = True
except ImportError:  # pragma: no cover
    _HAS_ST = False


STORAGE_DIR = Path(os.environ.get('LAI_STORAGE_DIR', Path.home() / '.local-ai'))
COLLECTIONS_FILE = STORAGE_DIR / 'collections.json'
EMBEDDINGS_FILE = STORAGE_DIR / 'embeddings.npy'
METADATA_FILE = STORAGE_DIR / 'metadata.json'

_DEFAULT_MODEL = 'all-MiniLM-L6-v2'
_MSG_INSTALL_ST = 'install sentence-transformers'


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
                (self.storage_dir / 'collections.json').read_text(encoding='utf-8')
            )
        except (FileNotFoundError, json.JSONDecodeError):
            self._collections = {}

        try:
            self._embeddings = np.load(self.storage_dir / 'embeddings.npy')
        except (FileNotFoundError, Exception):
            self._embeddings = None

        try:
            self._metadata = json.loads(
                (self.storage_dir / 'metadata.json').read_text(encoding='utf-8')
            )
        except (FileNotFoundError, json.JSONDecodeError):
            self._metadata = []

    def _save(self) -> None:
        (self.storage_dir / 'collections.json').write_text(
            json.dumps(self._collections, ensure_ascii=False, indent=2), encoding='utf-8'
        )
        if self._embeddings is not None:
            np.save(self.storage_dir / 'embeddings.npy', self._embeddings)
        (self.storage_dir / 'metadata.json').write_text(
            json.dumps(self._metadata, ensure_ascii=False, indent=2), encoding='utf-8'
        )

    def create_collection(self, name: str) -> None:
        if name in self._collections:
            return
        self._collections[name] = {
            'created_at': datetime.now(timezone.utc).isoformat(),
            'document_count': 0,
        }
        self._save()

    def list_collections(self) -> list[str]:
        return list(self._collections.keys())

    def delete_collection(self, name: str) -> bool:
        if name not in self._collections:
            return False
        del self._collections[name]
        keep_indices = [i for i, m in enumerate(self._metadata) if m.get('collection') != name]
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
                f'{collection}_{i}_{datetime.now(timezone.utc).timestamp()}'
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
                    'id': doc_id,
                    'collection': collection,
                    'text': documents[i],
                    'metadata': meta,
                    'index': start_idx + i,
                    'added_at': datetime.now(timezone.utc).isoformat(),
                }
            )

        self._collections[collection]['document_count'] = self._collections[collection].get(
            'document_count', 0
        ) + len(documents)
        self._save()
        return ids

    def add_file(self, collection: str, file_path: str | Path) -> str | None:
        path = Path(file_path)
        if not path.exists():
            return None
        text = path.read_text(encoding='utf-8', errors='ignore')
        if not text.strip():
            return None
        doc_id = f'{collection}:{path}'
        self.add_documents(
            collection,
            [text],
            metadatas=[{'file': str(path), 'type': path.suffix}],
            ids=[doc_id],
        )
        return doc_id

    def add_directory(
        self,
        collection: str,
        directory: str | Path,
        extensions: tuple[str, ...] = ('.py', '.md', '.txt', '.json', '.yaml', '.yml'),
    ) -> int:
        dir_path = Path(directory)
        count = 0
        for ext in extensions:
            for f in dir_path.rglob(f'*{ext}'):
                if self.add_file(collection, f):
                    count += 1
        return count

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
        with np.errstate(divide='ignore', invalid='ignore'):
            sims = (emb @ query_emb.T).flatten() / (norms.flatten() * qnorm.flatten())

        results: list[dict[str, Any]] = []
        for idx in np.argsort(sims)[::-1]:
            if sims[idx] < threshold:
                continue
            meta = self._metadata[int(idx)]
            if collection and meta.get('collection') != collection:
                continue
            results.append(
                {
                    'id': meta['id'],
                    'collection': meta['collection'],
                    'text': meta['text'],
                    'metadata': meta.get('metadata', {}),
                    'score': float(sims[idx]),
                }
            )
            if len(results) >= top_k:
                break
        return results

    def get_collection_info(self, name: str) -> dict[str, Any] | None:
        return self._collections.get(name)

    def stats(self) -> dict[str, Any]:
        return {
            'collections': len(self._collections),
            'total_documents': len(self._metadata),
            'embedding_dim': int(self._embeddings.shape[1]) if self._embeddings is not None else 0,
            'model': self.model_name,
            'storage_dir': str(self.storage_dir),
        }


def get_rag() -> LocalRAG:
    if not hasattr(get_rag, '_instance'):
        get_rag._instance = LocalRAG()  # type: ignore[attr-defined]
    return get_rag._instance  # type: ignore[return-value]
