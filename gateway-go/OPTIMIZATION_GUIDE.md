# Gateway Optimization Guide - Best Practices from Industry Leaders

Based on research from: [Portkey AI Gateway](https://github.com/Portkey-AI/gateway), [vLLM](https://github.com/vllm-project/vllm), [SGLang](https://github.com/sgl-project/sglang), [HuggingFace TGI](https://github.com/huggingface/text-generation-inference), [llama.cpp](https://github.com/ggml-org/llama.cpp)

---

## 1. LLM Server Optimization (llama.cpp)

### GPU Layer Offloading
```bash
# Offload all layers to GPU (best for RTX 3070 8GB with Qwen3-8B-Q4_K_M)
-ngl 99  # or -ngl all
```

### KV Cache Quantization (FP8 - HUGE VRAM SAVINGS)
```bash
# FP8 KV cache - saves ~50% KV cache VRAM with minimal quality loss
-ctk f8_e4m3 -ctv f8_e4m3
# Alternative: q8_0 (8-bit) - slightly more VRAM, wider compatibility
-ctk q8_0 -ctv q8_0
```

### Context Checkpoints (for long contexts)
```bash
# Enable context checkpoints for 16K+ contexts
--ctx-checkpoints 8 --checkpoint-min-step 4096
```

### Flash Attention + Continuous Batching
```bash
--flash-attn on --cont-batching
```

### Speculative Decoding (n-gram - zero VRAM overhead)
```bash
--spec-type ngram-simple --spec-ngram-simple-size-n 12 --spec-ngram-simple-size-m 48
```

### Batch Sizing
```bash
-b 4096 -ub 512  # logical max batch, physical max batch
```

### Complete Optimized llama-server Command
```bash
./llama-server -m /path/to/model.gguf \
  --host 0.0.0.0 --port 8080 \
  -c 32768 -ngl 99 \
  --flash-attn on --cont-batching \
  -ctk f8_e4m3 -ctv f8_e4m3 \
  --ctx-checkpoints 8 --checkpoint-min-step 4096 \
  --spec-type ngram-simple --spec-ngram-simple-size-n 12 --spec-ngram-simple-size-m 48 \
  -b 4096 -ub 512 \
  --alias my-model
```

---

## 2. Gateway Architecture Best Practices

### From Portkey AI Gateway (<1ms latency, 10B+ tokens/day)

**Core Principles:**
- **<1ms gateway latency** - proxy overhead must be negligible
- **Streaming-first** - SSE support mandatory for LLM streaming
- **Zero-copy** where possible - avoid body buffering
- **Connection pooling** - reuse upstream connections

### Gateway Implementation Checklist

| Feature | Implementation | Priority |
|---------|----------------|----------|
| **Reverse Proxy** | `httputil.ReverseProxy` with custom `Rewrite` | ✅ Done |
| **Streaming SSE** | `gin-contrib/sse` + custom flush | ✅ Done |
| **Connection Pooling** | `http.Transport` with `MaxIdleConns` | ✅ Done |
| **Request/Response Compression** | gzip for JSON bodies >1KB | 🔄 Add |
| **Smart Caching** | LRU + semantic cache (embeddings) | ✅ LRU Done |
| **Circuit Breaker** | Fail fast on upstream errors | 🔄 Add |
| **Load Balancing** | Weighted round-robin | 🔄 Add |
| **Request Timeout** | Per-upstream configurable | ✅ Done |
| **Rate Limiting** | Token bucket / sliding window | 🔄 Add |
| **Metrics** | Prometheus + custom business metrics | ✅ Done |

---

## 3. Context Optimization Techniques

### From vLLM / SGLang Research

| Technique | Description | Implementation |
|-----------|-------------|----------------|
| **PagedAttention** | vLLM's breakthrough - paginated KV cache | llama.cpp handles internally |
| **Continuous Batching** | Add new requests mid-batch | llama.cpp `--cont-batching` |
| **Chunked Prefill** | Split large prefill across iterations | llama.cpp auto |
| **Prefix Caching** | Cache common prefixes | llama.cpp `--ctx-checkpoints` |
| **Speculative Decoding** | Draft model predicts, target verifies | llama.cpp `--spec-type` |

### Gateway-Level Context Compression

```go
// Smart compression preserving code blocks
func compressContent(text string, preserveCode bool) string {
    // 1. Extract code blocks (``` ``` and ` `)
    // 2. Compress non-code: whitespace, newlines
    // 3. Restore code blocks
    // Result: 30-50% token reduction
}
```

### Intelligent Context Truncation

```go
// Priority order for truncation:
// 1. ALWAYS keep last user message
// 2. Keep recent assistant messages
// 3. Keep system prompt (if small)
// 4. Drop oldest user/assistant pairs first
// 5. Never truncate mid-code-block
```

---

## 4. Caching Strategies

### From Portkey & vLLM Research

| Cache Type | Use Case | Implementation |
|------------|----------|----------------|
| **Exact Match** | Identical prompts | LRU with SHA256 key |
| **Semantic Cache** | Similar prompts (embeddings) | Vector DB (Redis + embeddings) |
| **Prefix Cache** | Common prefixes | llama.cpp checkpoints |
| **KV Cache** | Model internal | llama.cpp `--ctx-checkpoints` |

### Recommended Cache Config

```yaml
cache:
  enabled: true
  max_entries: 5000
  ttl: 1h
  max_key_size: 1024
  # For semantic cache (future):
  # semantic:
  #   enabled: true
  #   threshold: 0.95
  #   embedding_model: "sentence-transformers/all-MiniLM-L6-v2"
```

---

## 5. Streaming Optimization

### From TGI / vLLM / SGLang

```go
// Critical for streaming:
// 1. Disable buffering: FlushInterval = 100ms
proxy.FlushInterval = 100 * time.Millisecond

// 2. Proper headers for SSE
c.Header("Content-Type", "text/event-stream")
c.Header("Cache-Control", "no-cache")
c.Header("Connection", "keep-alive")
c.Header("X-Accel-Buffering", "no")  // Disable nginx buffering

// 3. Chunked transfer encoding
// 4. Proper backpressure handling
// 5. Graceful cancellation on client disconnect
```

### Gateway Streaming Implementation

```go
func (g *Gateway) handleStream(c *gin.Context) {
    c.Header("Content-Type", "text/event-stream")
    c.Header("Cache-Control", "no-cache")
    c.Header("Connection", "keep-alive")
    c.Header("X-Accel-Buffering", "no")

    c.Stream(func(w io.Writer) bool {
        // Forward chunks immediately with minimal buffering
        // Track latency per chunk
        // Handle client disconnect gracefully
    })
}
```

---

## 6. Load Balancing & Reliability

### From Portkey & Industry Standards

```go
// Circuit Breaker Pattern
type CircuitBreaker struct {
    failures       int
    lastFailure    time.Time
    state          State // Closed, Open, HalfOpen
    maxFailures    int
    timeout        time.Duration
}

// Retry with Exponential Backoff
func retryWithBackoff(fn func() error, maxRetries int) error {
    for i := 0; i < maxRetries; i++ {
        if err := fn(); err == nil {
            return nil
        }
        time.Sleep(time.Duration(math.Pow(2, float64(i))) * 100 * time.Millisecond)
    }
    return fmt.Errorf("max retries exceeded")
}

// Fallback Chain
// 1. Primary: llama.cpp (FP8, fast)
// 2. Fallback: llama.cpp (FP16, compatible)
// 3. Fallback: Ollama (qwen3:8b)
// 4. Fallback: Cloud API (last resort)
```

---

## 7. Monitoring & Observability

### Key Metrics (from Prometheus best practices)

```go
// RED Metrics (Rate, Errors, Duration)
requestsTotal = promauto.NewCounterVec(...)
requestDuration = promauto.NewHistogramVec(...)

// USE Metrics (Utilization, Saturation, Errors)
activeConnections = promauto.NewGauge(...)
queueDepth = promauto.NewGauge(...)

// Business Metrics
cacheHitRate = cacheHits / (cacheHits + cacheMisses)
tokensPerSecond = completionTokens / duration
costPerRequest = estimatedCost
```

### Health Check Endpoints

```go
// /health - Liveness (can serve traffic)
GET /health -> {"status":"ok","version":"3.0","upstream":"ok"}

// /ready - Readiness (upstream responsive)
GET /ready -> {"status":"ready","upstream_latency_ms":45}

// /metrics - Prometheus scrape
GET /metrics -> Prometheus format
```

---

## 8. Security & Compliance

From Portkey Enterprise & Industry Standards

```go
// Authentication
- Virtual keys (short-lived, revocable)
- Role-based access control (RBAC)
- API key rotation

// Guardrails (Portkey style)
- PII redaction (auto-detect SSN, emails, keys)
- Content filtering (deny/allow lists)
- Output validation (JSON schema, length limits)

// Compliance
- SOC2, HIPAA, GDPR, CCPA ready
- Audit logging (all requests/responses)
- Data residency controls
```

---

## 9. Configuration for Maximum Performance

### Optimized Gateway Config

```yaml
server:
  read_timeout: 30s
  write_timeout: 300s    # Long for streaming
  idle_timeout: 120s
  max_request_size: 50MB  # Large contexts

upstream:
  llama_cpp:
    timeout: 300s
    max_connections: 100  # Connection pool

context:
  max_tokens: 32768
  compression_threshold: 1024
  preserve_code_blocks: true

cache:
  max_entries: 5000
  ttl: 1h

filter:
  enabled: true  # Remove Kimi/Crush/OpenCode noise

metrics:
  enabled: true
  path: "/metrics"
```

---

## 10. Benchmarking & Continuous Optimization

### Benchmark Script

```bash
#!/bin/bash
# benchmark.sh - Run regularly to detect regressions

ENDPOINT="http://localhost:8081/v1/chat/completions"
MODEL="qwen3-8b"

echo "=== Latency Test ==="
for i in {1..10}; do
    curl -s -w "\nTotal: %{time_total}s\n" \
        -X POST $ENDPOINT \
        -H "Content-Type: application/json" \
        -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"/no_think hola"}],"max_tokens":50}'
done

echo "=== Throughput Test ==="
for i in {1..5}; do
    curl -s -X POST $ENDPOINT \
        -H "Content-Type: application/json" \
        -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"/no_think Escribe quicksort en Python"}],"max_tokens":500}' \
        | jq -r '.usage.completion_tokens / .timings.predicted_ms * 1000'
done

echo "=== Concurrent Test ==="
hey -n 100 -c 10 -m POST \
    -H "Content-Type: application/json" \
    -d '{"model":"qwen3-8b","messages":[{"role":"user","content":"/no_think hola"}],"max_tokens":20}' \
    $ENDPOINT
```

### Regression Detection

```yaml
# CI/CD integration
benchmarks:
  p50_latency: < 50ms
  p99_latency: < 200ms
  throughput: > 70 tok/s
  cache_hit_rate: > 30%
  error_rate: < 0.1%
```

---

## 11. Future Enhancements (Roadmap)

### Phase 1: Core Reliability (Week 1)
- [ ] Circuit breaker implementation
- [ ] Retry with exponential backoff
- [ ] Fallback chain (llama.cpp → Ollama → Cloud)
- [ ] Request/response compression (gzip)

### Phase 2: Smart Caching (Week 2)
- [ ] Semantic cache with embeddings
- [ ] Cache warming on startup
- [ ] Cache analytics dashboard

### Phase 3: Advanced Routing (Week 3)
- [ ] Weighted load balancing
- [ ] Conditional routing (model selection)
- [ ] Request prioritization

### Phase 4: Enterprise Features (Week 4)
- [ ] Virtual keys / RBAC
- [ ] Guardrails (PII, content filter)
- [ ] Audit logging
- [ ] Multi-tenancy

---

## 11. Key Takeaways

| Area | Best Practice | Impact |
|------|---------------|--------|
| **llama.cpp** | FP8 KV cache + checkpoints | 50% VRAM saved, long context |
| **Gateway** | Go + gin + streaming SSE | <1ms overhead, 10k+ conn |
| **Caching** | LRU + semantic (future) | 30-50% cost reduction |
| **Context** | Smart truncation + code preserve | Fit 32K in 16K model |
| **Streaming** | SSE + flush 100ms | Perceived latency < 100ms |
| **Reliability** | Circuit breaker + fallback | 99.9% uptime |
| **Observability** | RED + USE metrics | Fast debugging |

---

## References

1. [Portkey AI Gateway](https://github.com/Portkey-AI/gateway) - Production AI Gateway
2. [vLLM](https://github.com/vllm-project/vllm) - PagedAttention, continuous batching
3. [SGLang](https://github.com/sgl-project/sglang) - RL-optimized serving
4. [HuggingFace TGI](https://github.com/huggingface/text-generation-inference) - Production serving
5. [llama.cpp Performance](https://github.com/ggml-org/llama.cpp/wiki) - Local optimization
6. [FlashAttention](https://github.com/Dao-AILab/flash-attention) - Memory-efficient attention
6. [PagedAttention Paper](https://arxiv.org/abs/2309.06180) - vLLM core innovation
7. [Speculative Decoding](https://arxiv.org/abs/2211.17192) - 2-3x speedup

---

*Last updated: 2026-11-20 | Gateway v3.0 | llama.cpp b12567 | Qwen3-8B-Q4_K_M*