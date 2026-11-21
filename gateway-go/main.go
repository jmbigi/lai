package main

import (
	"bufio"
	"bytes"
	"context"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"math"
	"math/rand"
	"net/http"
	"net/http/httputil"
	"net/url"
	"os"
	"os/signal"
	"regexp"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"github.com/prometheus/client_golang/prometheus/promhttp"
	"gopkg.in/yaml.v3"
)

// ============================================================================
// CONFIGURATION
// ============================================================================

type Config struct {
	Server            ServerConfig            `yaml:"server"`
	Upstream          UpstreamConfig          `yaml:"upstream"`
	Context           ContextConfig           `yaml:"context"`
	Cache             CacheConfig             `yaml:"cache"`
	Filter            FilterConfig            `yaml:"filter"`
	Metrics           MetricsConfig           `yaml:"metrics"`
	Logging           LoggingConfig           `yaml:"logging"`
	Compression       CompressionConfig       `yaml:"compression"`
	CircuitBreaker    CircuitBreakerConfig    `yaml:"circuit_breaker"`
	Auth              AuthConfig              `yaml:"auth"`
	RateLimit         RateLimitConfig         `yaml:"rate_limit"`
	MTLS              MTLSConfig              `yaml:"mtls"`
}

type ServerConfig struct {
	Host            string        `yaml:"host"`
	Port            int           `yaml:"port"`
	ReadTimeout     time.Duration `yaml:"read_timeout"`
	WriteTimeout    time.Duration `yaml:"write_timeout"`
	IdleTimeout     time.Duration `yaml:"idle_timeout"`
	MaxRequestSize  int64         `yaml:"max_request_size"`
}

type UpstreamConfig struct {
	LlamaCPP UpstreamServer `yaml:"llama_cpp"`
	Ollama   UpstreamServer `yaml:"ollama"`
	Fallback []UpstreamServer `yaml:"fallback"`
}

type UpstreamServer struct {
	Name             string        `yaml:"name"`
	URL              string        `yaml:"url"`
	Weight           int           `yaml:"weight"`
	Timeout          time.Duration `yaml:"timeout"`
	MaxConnections   int           `yaml:"max_connections"`
	ModelOverrides   []string      `yaml:"model_overrides"`
}

type ContextConfig struct {
	MaxTokens             int  `yaml:"max_tokens"`
	MaxOutputTokens       int  `yaml:"max_output_tokens"`
	CompressionThreshold  int  `yaml:"compression_threshold"`
	PreserveCodeBlocks    bool `yaml:"preserve_code_blocks"`
	SemanticCompression   bool `yaml:"semantic_compression"`
}

type CacheConfig struct {
	Enabled      bool          `yaml:"enabled"`
	MaxEntries   int           `yaml:"max_entries"`
	TTL          time.Duration `yaml:"ttl"`
	MaxKeySize   int           `yaml:"max_key_size"`
	Semantic     bool          `yaml:"semantic"`
}

type FilterConfig struct {
	Enabled  bool     `yaml:"enabled"`
	Keywords []string `yaml:"keywords"`
}

type MetricsConfig struct {
	Enabled bool   `yaml:"enabled"`
	Path    string `yaml:"path"`
}

type LoggingConfig struct {
	Level      string `yaml:"level"`
	Format     string `yaml:"format"`
	AccessLog  bool   `yaml:"access_log"`
}

type CompressionConfig struct {
	Enabled           bool    `yaml:"enabled"`
	Threshold         int     `yaml:"threshold"`
	PreserveCodeBlocks bool   `yaml:"preserve_code_blocks"`
	MinCompressionRatio float64 `yaml:"min_compression_ratio"`
	Methods           []string `yaml:"methods"`
}

type CircuitBreakerConfig struct {
	Enabled          bool          `yaml:"enabled"`
	MaxFailures      int           `yaml:"max_failures"`
	Timeout          time.Duration `yaml:"timeout"`
	HalfOpenRequests int           `yaml:"half_open_requests"`
}

type AuthConfig struct {
	Enabled            bool     `yaml:"enabled"`
	OpenKey            string   `yaml:"open_key"`
	ValidKeys          []string `yaml:"valid_keys"`
	RateLimit          RateLimitAuthConfig `yaml:"rate_limit"`
}

type RateLimitAuthConfig struct {
	Enabled           bool `yaml:"enabled"`
	RequestsPerMinute int  `yaml:"requests_per_minute"`
}

type RateLimitConfig struct {
	Enabled           bool `yaml:"enabled"`
	RequestsPerMinute int  `yaml:"requests_per_minute"`
}

type MTLSConfig struct {
	Enabled           bool   `yaml:"enabled"`
	CaCert            string `yaml:"ca_cert"`
	ServerCert        string `yaml:"server_cert"`
	ServerKey         string `yaml:"server_key"`
	RequireClientCert bool   `yaml:"require_client_cert"`
}

type clientInfo struct {
	count       int
	windowStart time.Time
}

type CircuitBreakerState int

const (
	CircuitClosed CircuitBreakerState = iota
	CircuitOpen
	CircuitHalfOpen
)

type CircuitBreaker struct {
	name             string
	state            CircuitBreakerState
	failures         int
	successes        int
	lastFailure      time.Time
	maxFailures      int
	timeout          time.Duration
	halfOpenRequests int
	halfOpenSuccesses int
	mu               sync.RWMutex
}

func NewCircuitBreaker(name string, config CircuitBreakerConfig) *CircuitBreaker {
	return &CircuitBreaker{
		name:             name,
		state:            CircuitClosed,
		maxFailures:      config.MaxFailures,
		timeout:          config.Timeout,
		halfOpenRequests: config.HalfOpenRequests,
	}
}

func (cb *CircuitBreaker) Allow() bool {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	switch cb.state {
	case CircuitClosed:
		return true
	case CircuitOpen:
		if time.Since(cb.lastFailure) > cb.timeout {
			cb.state = CircuitHalfOpen
			cb.halfOpenSuccesses = 0
			return true
		}
		return false
	case CircuitHalfOpen:
		return cb.halfOpenSuccesses < cb.halfOpenRequests
	}
	return false
}

func (cb *CircuitBreaker) RecordSuccess() {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	switch cb.state {
	case CircuitClosed:
		cb.failures = 0
	case CircuitHalfOpen:
		cb.halfOpenSuccesses++
		if cb.halfOpenSuccesses >= cb.halfOpenRequests {
			cb.state = CircuitClosed
			cb.failures = 0
			cb.halfOpenSuccesses = 0
		}
	}
}

func (cb *CircuitBreaker) RecordFailure() {
	cb.mu.Lock()
	defer cb.mu.Unlock()

	cb.failures++
	cb.lastFailure = time.Now()

	switch cb.state {
	case CircuitClosed:
		if cb.failures >= 5 { // Default max failures
			cb.state = CircuitOpen
		}
	case CircuitHalfOpen:
		cb.state = CircuitOpen
	}
}

func (cb *CircuitBreaker) State() CircuitBreakerState {
	cb.mu.RLock()
	defer cb.mu.RUnlock()
	return cb.state
}

// ============================================================================
// REQUEST/RESPONSE TYPES (OpenAI 2.3.0 compatible)
// ============================================================================

type Message struct {
	Role         string          `json:"role"`
	Content      json.RawMessage `json:"content"`
	Name         string          `json:"name,omitempty"`
	ToolCalls    []ToolCall      `json:"tool_calls,omitempty"`
	ToolCallID   string          `json:"tool_call_id,omitempty"`
	FunctionCall *FunctionCall   `json:"function_call,omitempty"`
}

type ContentPart struct {
	Type     string          `json:"type"`
	Text     string          `json:"text,omitempty"`
	ImageURL *ImageURL       `json:"image_url,omitempty"`
	Audio    *AudioData      `json:"audio,omitempty"`
	File     *FileData       `json:"file,omitempty"`
}

type ImageURL struct {
	URL    string `json:"url"`
	Detail string `json:"detail,omitempty"`
}

type AudioData struct {
	Data   string `json:"data"`
	Format string `json:"format,omitempty"`
}

type FileData struct {
	Filename string `json:"filename"`
	FileData string `json:"file_data"`
}

type FunctionCall struct {
	Name      string `json:"name"`
	Arguments string `json:"arguments"`
}

type ToolCall struct {
	ID       string      `json:"id"`
	Type     string      `json:"type"`
	Function FunctionCall `json:"function"`
}

type Tool struct {
	Type     string           `json:"type"`
	Function FunctionDefinition `json:"function"`
}

type FunctionDefinition struct {
	Name        string                 `json:"name"`
	Description string                 `json:"description,omitempty"`
	Parameters  map[string]interface{} `json:"parameters"`
	Strict      bool                   `json:"strict,omitempty"`
}

type ChatCompletionRequest struct {
	Model             string          `json:"model"`
	Messages          []Message       `json:"messages"`
	Temperature       *float64        `json:"temperature,omitempty"`
	TopP              *float64        `json:"top_p,omitempty"`
	TopK              *int            `json:"top_k,omitempty"`
	N                 int             `json:"n,omitempty"`
	Stream            *bool           `json:"stream,omitempty"`
	StreamOptions     *StreamOptions  `json:"stream_options,omitempty"`
	Stop              json.RawMessage `json:"stop,omitempty"`
	MaxTokens         *int            `json:"max_tokens,omitempty"`
	MaxCompletionTokens *int        `json:"max_completion_tokens,omitempty"`
	PresencePenalty   *float64        `json:"presence_penalty,omitempty"`
	FrequencyPenalty  *float64        `json:"frequency_penalty,omitempty"`
	RepetitionPenalty *float64        `json:"repetition_penalty,omitempty"`
	LogitBias         map[string]int  `json:"logit_bias,omitempty"`
	Logprobs          *bool           `json:"logprobs,omitempty"`
	TopLogprobs       *int            `json:"top_logprobs,omitempty"`
	User              string          `json:"user,omitempty"`
	Tools             []Tool          `json:"tools,omitempty"`
	ToolChoice        json.RawMessage `json:"tool_choice,omitempty"`
	ParallelToolCalls *bool           `json:"parallel_tool_calls,omitempty"`
	ChatTemplateKwargs map[string]interface{} `json:"chat_template_kwargs,omitempty"`
	ResponseFormat    *ResponseFormat `json:"response_format,omitempty"`
	Seed              *int64          `json:"seed,omitempty"`
	StopTokenIDs      []int           `json:"stop_token_ids,omitempty"`
	MinP              *float64        `json:"min_p,omitempty"`
	TypicalP          *float64        `json:"typical_p,omitempty"`
	Mirostat          *int            `json:"mirostat,omitempty"`
	MirostatEta       *float64        `json:"mirostat_eta,omitempty"`
	MirostatTau       *float64        `json:"mirostat_tau,omitempty"`
	Grammar           string          `json:"grammar,omitempty"`
	JSONSchema        map[string]interface{} `json:"json_schema,omitempty"`
	MinTokens         *int            `json:"min_tokens,omitempty"`
	Priority          int             `json:"priority,omitempty"`
}

type StreamOptions struct {
	IncludeUsage bool `json:"include_usage"`
}

type ResponseFormat struct {
	Type       string                 `json:"type"`
	JSONSchema *JSONSchemaFormat      `json:"json_schema,omitempty"`
}

type JSONSchemaFormat struct {
	Name   string                 `json:"name"`
	Strict bool                   `json:"strict"`
	Schema map[string]interface{} `json:"schema"`
}

type ChatCompletionResponse struct {
	ID                string         `json:"id"`
	Object            string         `json:"object"`
	Created           int64          `json:"created"`
	Model             string         `json:"model"`
	Choices           []ChatChoice   `json:"choices"`
	Usage             *Usage         `json:"usage,omitempty"`
	SystemFingerprint string         `json:"system_fingerprint,omitempty"`
}

type ChatChoice struct {
	Index        int      `json:"index"`
	Message      Message  `json:"message"`
	FinishReason string   `json:"finish_reason"`
	Logprobs     *LogProbs `json:"logprobs,omitempty"`
}

type LogProbs struct {
	Content []TokenLogProb `json:"content"`
}

type TokenLogProb struct {
	Token       string        `json:"token"`
	Logprob     float64       `json:"logprob"`
	Bytes       []int         `json:"bytes"`
	TopLogprobs []TopLogProb  `json:"top_logprobs,omitempty"`
}

type TopLogProb struct {
	Token   string  `json:"token"`
	Logprob float64 `json:"logprob"`
	Bytes   []int   `json:"bytes"`
}

type Usage struct {
	PromptTokens     int                    `json:"prompt_tokens"`
	CompletionTokens int                    `json:"completion_tokens"`
	TotalTokens      int                    `json:"total_tokens"`
	PromptTokensDetails *PromptTokensDetails `json:"prompt_tokens_details,omitempty"`
	CompletionTokensDetails *CompletionTokensDetails `json:"completion_tokens_details,omitempty"`
}

type PromptTokensDetails struct {
	CachedTokens int `json:"cached_tokens"`
	AudioTokens  int `json:"audio_tokens"`
}

type CompletionTokensDetails struct {
	ReasoningTokens int `json:"reasoning_tokens"`
	AudioTokens     int `json:"audio_tokens"`
}

type ChatCompletionChunk struct {
	ID                string         `json:"id"`
	Object            string         `json:"object"`
	Created           int64          `json:"created"`
	Model             string         `json:"model"`
	Choices           []ChunkChoice  `json:"choices"`
	Usage             *Usage         `json:"usage,omitempty"`
	SystemFingerprint string         `json:"system_fingerprint,omitempty"`
}

type ChunkChoice struct {
	Index        int          `json:"index"`
	Delta        DeltaMessage `json:"delta"`
	FinishReason *string      `json:"finish_reason"`
	Logprobs     *LogProbs    `json:"logprobs,omitempty"`
}

type DeltaMessage struct {
	Role      string     `json:"role,omitempty"`
	Content   *string    `json:"content,omitempty"`
	ToolCalls []DeltaToolCall `json:"tool_calls,omitempty"`
}

type DeltaToolCall struct {
	Index    int    `json:"index"`
	ID       string `json:"id,omitempty"`
	Type     string `json:"type,omitempty"`
	Function DeltaFunction `json:"function,omitempty"`
}

type DeltaFunction struct {
	Name      string `json:"name,omitempty"`
	Arguments string `json:"arguments,omitempty"`
}

type ModelList struct {
	Object string  `json:"object"`
	Data   []Model `json:"data"`
}

type Model struct {
	ID      string `json:"id"`
	Object  string `json:"object"`
	Created int64  `json:"created"`
	OwnedBy string `json:"owned_by"`
	Permission []interface{} `json:"permission,omitempty"`
	Root    string `json:"root,omitempty"`
	Parent  string `json:"parent,omitempty"`
	MaxTokens int `json:"max_tokens,omitempty"`
	ContextWindow int `json:"context_window,omitempty"`
	Architecture string `json:"architecture,omitempty"`
	Quantization string `json:"quantization,omitempty"`
	Capabilities []string `json:"capabilities,omitempty"`
}

type EmbeddingRequest struct {
	Model string `json:"model"`
	Input interface{} `json:"input"`
	EncodingFormat string `json:"encoding_format,omitempty"`
	Dimensions *int `json:"dimensions,omitempty"`
	User string `json:"user,omitempty"`
}

type EmbeddingResponse struct {
	Object string `json:"object"`
	Data []Embedding `json:"data"`
	Model string `json:"model"`
	Usage Usage `json:"usage"`
}

type Embedding struct {
	Object string `json:"object"`
	Index int `json:"index"`
	Embedding []float64 `json:"embedding"`
}

type ErrorResponse struct {
	Error ErrorDetail `json:"error"`
}

type ErrorDetail struct {
	Message string `json:"message"`
	Type    string `json:"type"`
	Code    string `json:"code"`
	Param   string `json:"param,omitempty"`
}

// ============================================================================
// GATEWAY IMPLEMENTATION
// ============================================================================

type Gateway struct {
	config           *Config
	cache            *LRUCache
	upstreams        map[string]*httputil.ReverseProxy
	client           *http.Client
	filterRegex      *regexp.Regexp
	semanticCache    *SemanticCache
	metrics          *Metrics
	circuitBreakers  map[string]*CircuitBreaker
	upstreamConfigs  map[string]UpstreamServer
	requestCounts    map[string]int64
	mu               sync.RWMutex
}

type LRUCache struct {
	mu       sync.RWMutex
	entries  map[string]*cacheEntry
	maxSize  int
	ttl      time.Duration
}

type cacheEntry struct {
	value      []byte
	expiresAt  time.Time
	accessTime time.Time
}

func NewLRUCache(maxSize int, ttl time.Duration) *LRUCache {
	c := &LRUCache{
		entries: make(map[string]*cacheEntry),
		maxSize: maxSize,
		ttl:     ttl,
	}
	go c.cleanupLoop()
	return c
}

func (c *LRUCache) Get(key string) ([]byte, bool) {
	c.mu.RLock()
	entry, ok := c.entries[key]
	c.mu.RUnlock()
	if !ok {
		return nil, false
	}
	if time.Now().After(entry.expiresAt) {
		c.mu.Lock()
		delete(c.entries, key)
		c.mu.Unlock()
		return nil, false
	}
	c.mu.Lock()
	entry.accessTime = time.Now()
	c.mu.Unlock()
	return entry.value, true
}

func (c *LRUCache) Set(key string, value []byte) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if len(c.entries) >= c.maxSize {
		c.evictLRU()
	}
	c.entries[key] = &cacheEntry{
		value:      value,
		expiresAt:  time.Now().Add(c.ttl),
		accessTime: time.Now(),
	}
}

func (c *LRUCache) evictLRU() {
	var oldestKey string
	var oldestTime time.Time
	first := true
	for k, v := range c.entries {
		if first || v.accessTime.Before(oldestTime) {
			oldestKey = k
			oldestTime = v.accessTime
			first = false
		}
	}
	if oldestKey != "" {
		delete(c.entries, oldestKey)
	}
}

func (c *LRUCache) cleanupLoop() {
	ticker := time.NewTicker(5 * time.Minute)
	defer ticker.Stop()
	for range ticker.C {
		c.mu.Lock()
		now := time.Now()
		for k, v := range c.entries {
			if now.After(v.expiresAt) {
				delete(c.entries, k)
			}
		}
		c.mu.Unlock()
	}
}

type SemanticCache struct {
	mu       sync.RWMutex
	entries  map[string]*semanticEntry
	maxSize  int
}

type semanticEntry struct {
	key        string
	embedding  []float32
	response   []byte
	expiresAt  time.Time
}

func NewSemanticCache(maxSize int) *SemanticCache {
	return &SemanticCache{
		entries: make(map[string]*semanticEntry),
		maxSize: maxSize,
	}
}

func (s *SemanticCache) Get(embedding []float32, threshold float64) ([]byte, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	
	var bestEntry *semanticEntry
	bestScore := 0.0
	
	for _, entry := range s.entries {
		if time.Now().After(entry.expiresAt) {
			continue
		}
		score := cosineSimilarity(embedding, entry.embedding)
		if score > threshold && score > bestScore {
			bestScore = score
			bestEntry = entry
		}
	}
	
	if bestEntry != nil {
		return bestEntry.response, true
	}
	return nil, false
}

func (s *SemanticCache) Set(embedding []float32, response []byte) {
	s.mu.Lock()
	defer s.mu.Unlock()
	
	if len(s.entries) >= s.maxSize {
		// Evict oldest
		var oldestKey string
		var oldestTime time.Time
		first := true
		for k, v := range s.entries {
			if first || v.expiresAt.Before(oldestTime) {
				oldestKey = k
				oldestTime = v.expiresAt
				first = false
			}
		}
		if oldestKey != "" {
			delete(s.entries, oldestKey)
		}
	}
	
	key := fmt.Sprintf("%x", sha256.Sum256(float32SliceToBytes(embedding)))[:16]
	s.entries[key] = &semanticEntry{
		embedding: embedding,
		response:  response,
		expiresAt: time.Now().Add(time.Hour),
	}
}

func cosineSimilarity(a, b []float32) float64 {
	if len(a) != len(b) {
		return 0
	}
	var dot, normA, normB float64
	for i := range a {
		dot += float64(a[i] * b[i])
		normA += float64(a[i] * a[i])
		normB += float64(b[i] * b[i])
	}
	if normA == 0 || normB == 0 {
		return 0
	}
	return dot / (math.Sqrt(normA) * math.Sqrt(normB))
}

func float32SliceToBytes(s []float32) []byte {
	buf := make([]byte, len(s)*4)
	for i, v := range s {
		bits := math.Float32bits(v)
		buf[i*4] = byte(bits)
		buf[i*4+1] = byte(bits >> 8)
		buf[i*4+2] = byte(bits >> 16)
		buf[i*4+3] = byte(bits >> 24)
	}
	return buf
}

// ============================================================================
// METRICS
// ============================================================================

type Metrics struct {
	requestsTotal      prometheus.CounterVec
	requestDuration    prometheus.HistogramVec
	cacheHits          prometheus.Counter
	cacheMisses        prometheus.Counter
	semanticCacheHits  prometheus.Counter
	upstreamLatency    prometheus.HistogramVec
	activeConnections  prometheus.Gauge
	requestSize        prometheus.Histogram
	responseSize       prometheus.Histogram
	compressionRatio   prometheus.Histogram
	tokensPerSecond    prometheus.Histogram
}

func NewMetrics() *Metrics {
	m := &Metrics{}
	m.requestsTotal = *promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "gateway_requests_total",
			Help: "Total number of requests",
		},
		[]string{"method", "endpoint", "status"},
	)
	m.requestDuration = *promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "gateway_request_duration_seconds",
			Help:    "Request duration in seconds",
			Buckets: prometheus.DefBuckets,
		},
		[]string{"method", "endpoint"},
	)
	m.cacheHits = promauto.NewCounter(
		prometheus.CounterOpts{
			Name: "gateway_cache_hits_total",
			Help: "Total cache hits",
		},
	)
	m.cacheMisses = promauto.NewCounter(
		prometheus.CounterOpts{
			Name: "gateway_cache_misses_total",
			Help: "Total cache misses",
		},
	)
	m.semanticCacheHits = promauto.NewCounter(
		prometheus.CounterOpts{
			Name: "gateway_semantic_cache_hits_total",
			Help: "Total semantic cache hits",
		},
	)
	m.upstreamLatency = *promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "gateway_upstream_latency_seconds",
			Help:    "Upstream latency in seconds",
			Buckets: prometheus.DefBuckets,
		},
		[]string{"upstream"},
	)
	m.activeConnections = promauto.NewGauge(
		prometheus.GaugeOpts{
			Name: "gateway_active_connections",
			Help: "Number of active connections",
		},
	)
	m.requestSize = promauto.NewHistogram(
		prometheus.HistogramOpts{
			Name:    "gateway_request_size_bytes",
			Help:    "Request size in bytes",
			Buckets: []float64{100, 1000, 10000, 100000, 1000000, 10000000},
		},
	)
	m.responseSize = promauto.NewHistogram(
		prometheus.HistogramOpts{
			Name:    "gateway_response_size_bytes",
			Help:    "Response size in bytes",
			Buckets: []float64{100, 1000, 10000, 100000, 1000000, 10000000},
		},
	)
	m.compressionRatio = promauto.NewHistogram(
		prometheus.HistogramOpts{
			Name:    "gateway_compression_ratio",
			Help:    "Compression ratio achieved",
			Buckets: []float64{0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0},
		},
	)
	m.tokensPerSecond = promauto.NewHistogram(
		prometheus.HistogramOpts{
			Name:    "gateway_tokens_per_second",
			Help:    "Tokens generated per second",
			Buckets: []float64{10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 150, 200},
		},
	)
	return m
}

// ============================================================================
// GATEWAY CORE
// ============================================================================

func NewGateway(config *Config) (*Gateway, error) {
	g := &Gateway{
		config:          config,
		cache:           NewLRUCache(config.Cache.MaxEntries, config.Cache.TTL),
		upstreams:       make(map[string]*httputil.ReverseProxy),
		circuitBreakers: make(map[string]*CircuitBreaker),
		upstreamConfigs: make(map[string]UpstreamServer),
		requestCounts:   make(map[string]int64),
		client: &http.Client{
			Timeout: config.Upstream.LlamaCPP.Timeout,
			Transport: &http.Transport{
				MaxIdleConns:        config.Upstream.LlamaCPP.MaxConnections,
				MaxIdleConnsPerHost: config.Upstream.LlamaCPP.MaxConnections,
				IdleConnTimeout:     90 * time.Second,
				DisableCompression:  false,
				ForceAttemptHTTP2:   true,
			},
		},
		metrics: NewMetrics(),
	}

	if config.Cache.Semantic {
		g.semanticCache = NewSemanticCache(config.Cache.MaxEntries / 10)
	}

	if config.Filter.Enabled && len(config.Filter.Keywords) > 0 {
		pattern := `(?i)` + strings.Join(config.Filter.Keywords, "|")
		re, err := regexp.Compile(pattern)
		if err != nil {
			return nil, fmt.Errorf("invalid filter regex: %w", err)
		}
		g.filterRegex = re
	}

	// Initialize circuit breakers for each upstream
	for name, upstream := range map[string]UpstreamServer{
		"llama_cpp": config.Upstream.LlamaCPP,
		"ollama":    config.Upstream.Ollama,
	} {
		if upstream.URL == "" {
			continue
		}
		g.upstreamConfigs[name] = upstream
		g.circuitBreakers[name] = NewCircuitBreaker(name, config.CircuitBreaker)
	}

	// Create reverse proxies for primary upstreams
	for name, upstream := range map[string]UpstreamServer{
		"llama_cpp": config.Upstream.LlamaCPP,
		"ollama":    config.Upstream.Ollama,
	} {
		for _, fb := range config.Upstream.Fallback {
			if fb.Name == name {
				continue
			}
		}
		if upstream.URL == "" {
			continue
		}
		targetURL, err := url.Parse(upstream.URL)
		if err != nil {
			return nil, fmt.Errorf("invalid upstream URL %s: %w", upstream.URL, err)
		}
		proxy := httputil.NewSingleHostReverseProxy(targetURL)
		proxy.Transport = &http.Transport{
			MaxIdleConns:        upstream.MaxConnections,
			MaxIdleConnsPerHost: upstream.MaxConnections,
			IdleConnTimeout:     90 * time.Second,
			DisableCompression:  false,
			ForceAttemptHTTP2:   true,
		}
		proxy.FlushInterval = 50 * time.Millisecond
		proxy.ModifyResponse = func(resp *http.Response) error {
			return nil
		}
		g.upstreams[name] = proxy
	}

	// Add fallback upstreams
	for _, fb := range config.Upstream.Fallback {
		if fb.URL == "" {
			continue
		}
		targetURL, err := url.Parse(fb.URL)
		if err != nil {
			continue
		}
		proxy := httputil.NewSingleHostReverseProxy(targetURL)
		proxy.Transport = &http.Transport{
			MaxIdleConns:        fb.MaxConnections,
			MaxIdleConnsPerHost: fb.MaxConnections,
			IdleConnTimeout:     90 * time.Second,
		}
		proxy.FlushInterval = 50 * time.Millisecond
		g.upstreams[fb.Name] = proxy
		g.upstreamConfigs[fb.Name] = fb
		g.circuitBreakers[fb.Name] = NewCircuitBreaker(fb.Name, config.CircuitBreaker)
	}

	return g, nil
}

func (g *Gateway) cacheKey(req *ChatCompletionRequest) string {
	stream := false
	if req.Stream != nil {
		stream = *req.Stream
	}
	data, _ := json.Marshal(map[string]interface{}{
		"messages":      req.Messages,
		"temperature":   req.Temperature,
		"max_tokens":    req.MaxTokens,
		"model":         req.Model,
		"stream":        stream,
		"tools":         req.Tools,
		"tool_choice":   req.ToolChoice,
		"top_p":         req.TopP,
		"top_k":         req.TopK,
		"presence_pen":  req.PresencePenalty,
		"frequency_pen": req.FrequencyPenalty,
		"repetition_pen": req.RepetitionPenalty,
		"chat_tmpl_kw":  req.ChatTemplateKwargs,
	})
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])
}

func (g *Gateway) estimateTokens(text string) int {
	if text == "" {
		return 0
	}
	// More accurate for code: ~3.5 chars per token
	return max(1, len(text)/3)
}

func (g *Gateway) compressContent(text string) string {
	if text == "" || !g.config.Compression.Enabled {
		return text
	}
	if !g.config.Compression.PreserveCodeBlocks && !g.config.Context.PreserveCodeBlocks {
		text = regexp.MustCompile(`\s+`).ReplaceAllString(text, " ")
		text = regexp.MustCompile(`\n{3,}`).ReplaceAllString(text, "\n\n")
		return strings.TrimSpace(text)
	}

	codeBlocks := []string{}
	re := regexp.MustCompile("```[\\s\\S]*?```|`[^`]+`")
	text = re.ReplaceAllStringFunc(text, func(m string) string {
		codeBlocks = append(codeBlocks, m)
		return fmt.Sprintf("__CODE_BLOCK_%d__", len(codeBlocks)-1)
	})

	text = regexp.MustCompile(`[ \t]+`).ReplaceAllString(text, " ")
	text = regexp.MustCompile(`\n{3,}`).ReplaceAllString(text, "\n\n")
	text = strings.TrimSpace(text)

	for i, block := range codeBlocks {
		text = strings.Replace(text, fmt.Sprintf("__CODE_BLOCK_%d__", i), block, 1)
	}
	return text
}

func (g *Gateway) isSystemNoise(content string) bool {
	if g.filterRegex == nil || content == "" {
		return false
	}
	return g.filterRegex.MatchString(content)
}

func (g *Gateway) filterMessages(messages []Message) []Message {
	if !g.config.Filter.Enabled {
		return messages
	}

	filtered := make([]Message, 0, len(messages))
	for _, msg := range messages {
		// Never drop structural messages: system prompts, tool definitions or
		// results, and assistant turns that carry tool calls. Dropping these
		// breaks the OpenAI tool-calling contract.
		if msg.Role == "system" || msg.Role == "tool" || len(msg.ToolCalls) > 0 || msg.ToolCallID != "" {
			filtered = append(filtered, msg)
			continue
		}
		// Only drop short assistant messages that are entirely noise. Long
		// messages and user messages are preserved even if they contain a
		// keyword such as "function" or "parameters".
		if msg.Role == "assistant" {
			var content string
			if err := json.Unmarshal(msg.Content, &content); err == nil {
				trimmed := strings.TrimSpace(content)
				if trimmed != "" && len(trimmed) < 200 && g.isSystemNoise(trimmed) {
					continue
				}
			}
		}
		filtered = append(filtered, msg)
	}
	return filtered
}

func (g *Gateway) compressContext(messages []Message, maxTokens int) []Message {
	filtered := g.filterMessages(messages)
	if len(filtered) == 0 {
		return filtered
	}

	// Compress only plain-text system/user content. Tool calls, tool results
	// and multimodal content are left untouched.
	if g.config.Compression.Enabled {
		for i := range filtered {
			if filtered[i].Role != "system" && filtered[i].Role != "user" {
				continue
			}
			var content string
			if err := json.Unmarshal(filtered[i].Content, &content); err != nil {
				continue
			}
			content = g.compressContent(content)
			encoded, err := json.Marshal(content)
			if err == nil {
				filtered[i].Content = encoded
			}
		}
	}

	totalTokens := 0
	for _, msg := range filtered {
		var content string
		json.Unmarshal(msg.Content, &content)
		totalTokens += g.estimateTokens(content)
	}
	if totalTokens <= maxTokens {
		return filtered
	}

	// Do not trim a conversation that contains tool calls: dropping an
	// assistant/tool pair breaks the OpenAI tool-calling protocol.
	for _, msg := range filtered {
		if msg.Role == "tool" || len(msg.ToolCalls) > 0 || msg.ToolCallID != "" {
			return filtered
		}
	}

	result := make([]Message, 0, len(filtered))
	remaining := maxTokens
	lastUserIdx := -1
	for i := len(filtered) - 1; i >= 0; i-- {
		if filtered[i].Role == "user" {
			lastUserIdx = i
			break
		}
	}

	for i := len(filtered) - 1; i >= 0; i-- {
		msg := filtered[i]
		var content string
		json.Unmarshal(msg.Content, &content)
		msgTokens := g.estimateTokens(content)
		isLastUser := i == lastUserIdx
		isSystem := msg.Role == "system"
		if isLastUser || isSystem || msgTokens <= remaining {
			result = append([]Message{msg}, result...)
			remaining -= msgTokens
		} else {
			break
		}
	}
	return result
}

func (g *Gateway) transformRequest(req *ChatCompletionRequest) ChatCompletionRequest {
	if req.MaxTokens == nil {
		def := g.config.Context.MaxOutputTokens
		if def <= 0 {
			def = 4096
		}
		req.MaxTokens = &def
	}
	if req.Temperature == nil {
		def := 0.2
		req.Temperature = &def
	}
	if req.TopP == nil {
		def := 0.95
		req.TopP = &def
	}
	if req.Stream == nil {
		def := false
		req.Stream = &def
	}

	// Tools and tool_choice are part of the request contract. Never strip
	// them here: the model needs the schemas to call tools.
	if len(req.Messages) > 0 {
		req.Messages = g.compressContext(req.Messages, g.config.Context.MaxTokens)
	}
	return *req
}

// ============================================================================
// HANDLERS
// ============================================================================

func (g *Gateway) handleChat(c *gin.Context) {
	start := time.Now()
	g.metrics.activeConnections.Inc()
	defer g.metrics.activeConnections.Dec()

	var req ChatCompletionRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		g.metrics.requestsTotal.WithLabelValues("POST", "/v1/chat/completions", "400").Inc()
		c.JSON(http.StatusBadRequest, ErrorResponse{Error: ErrorDetail{Message: "Invalid request: " + err.Error(), Type: "invalid_request_error", Code: "invalid_request"}})
		return
	}

	requestSize := float64(c.Request.ContentLength)
	if requestSize < 0 {
		requestSize = 0
	}
	g.metrics.requestSize.Observe(requestSize)

	cacheKey := g.cacheKey(&req)
	if g.config.Cache.Enabled {
		if cached, ok := g.cache.Get(cacheKey); ok {
			g.metrics.cacheHits.Inc()
			c.Header("X-Cache", "HIT")
			c.Data(http.StatusOK, "application/json", cached)
			g.metrics.requestDuration.WithLabelValues("POST", "/v1/chat/completions").Observe(time.Since(start).Seconds())
			return
		}
		g.metrics.cacheMisses.Inc()
	}

	// Semantic cache
	if g.config.Cache.Semantic && g.semanticCache != nil {
		// Generate embedding for cache key (simplified - use hash)
		embedding := make([]float32, 384)
		hash := sha256.Sum256([]byte(cacheKey))
		for i := range embedding {
			embedding[i] = float32(hash[i%32]) / 255.0
		}
		if cached, ok := g.semanticCache.Get(embedding, 0.95); ok {
			g.metrics.semanticCacheHits.Inc()
			c.Header("X-Cache", "SEMANTIC_HIT")
			c.Data(http.StatusOK, "application/json", cached)
			g.metrics.requestDuration.WithLabelValues("POST", "/v1/chat/completions").Observe(time.Since(start).Seconds())
			return
		}
	}

	req = g.transformRequest(&req)

	// Select upstream with load balancing
	upstreamName := g.selectUpstream()
	proxy, ok := g.upstreams[upstreamName]
	if !ok {
		c.JSON(http.StatusServiceUnavailable, ErrorResponse{Error: ErrorDetail{Message: "No upstream available", Type: "server_error", Code: "no_upstream"}})
		return
	}

	body, _ := json.Marshal(req)
	upstreamReq, err := http.NewRequestWithContext(c.Request.Context(), "POST", g.config.Upstream.LlamaCPP.URL+"/v1/chat/completions", bytes.NewReader(body))
	if err != nil {
		c.JSON(http.StatusInternalServerError, ErrorResponse{Error: ErrorDetail{Message: "Failed to create upstream request", Type: "server_error", Code: "upstream_error"}})
		return
	}
	upstreamReq.Header.Set("Content-Type", "application/json")
	upstreamReq.Header.Set("Accept", "application/json")
	upstreamReq.Header.Set("User-Agent", "LocalAIGateway/3.0")

	isStream := req.Stream != nil && *req.Stream
	if isStream {
		g.handleStream(c, proxy, upstreamReq, start, cacheKey, upstreamName)
		return
	}

	var resp *http.Response

	upstreamStart := time.Now()

	// Check circuit breaker before request
	cb := g.circuitBreakers[upstreamName]
	if cb != nil && !cb.Allow() {
		// Circuit open, try fallback
		if fallbackResp, fbErr := g.tryFallback(upstreamReq); fbErr == nil {
			resp = fallbackResp
		} else {
			g.metrics.requestsTotal.WithLabelValues("POST", "/v1/chat/completions", "503").Inc()
			c.JSON(http.StatusServiceUnavailable, ErrorResponse{Error: ErrorDetail{Message: "Upstream circuit open, all fallbacks failed", Type: "server_error", Code: "circuit_open"}})
			return
		}
	} else {
		resp, err = g.client.Do(upstreamReq)
	}

	upstreamLatency := time.Since(upstreamStart)
	g.metrics.upstreamLatency.WithLabelValues(upstreamName).Observe(upstreamLatency.Seconds())

	if err != nil {
		// Record failure in circuit breaker
		if cb != nil {
			cb.RecordFailure()
		}
		// Try fallback
		if fallbackResp, fbErr := g.tryFallback(upstreamReq); fbErr == nil {
			resp = fallbackResp
			err = nil
		} else {
			g.metrics.requestsTotal.WithLabelValues("POST", "/v1/chat/completions", "502").Inc()
			c.JSON(http.StatusBadGateway, ErrorResponse{Error: ErrorDetail{Message: "Upstream error: " + err.Error(), Type: "server_error", Code: "upstream_error"}})
			return
		}
	} else {
		// Record success in circuit breaker
		if cb != nil {
			cb.RecordSuccess()
		}
	}
	defer resp.Body.Close()

	responseBody, err := io.ReadAll(resp.Body)
	if err != nil {
		c.JSON(http.StatusInternalServerError, ErrorResponse{Error: ErrorDetail{Message: "Failed to read response", Type: "server_error", Code: "read_error"}})
		return
	}

	g.metrics.responseSize.Observe(float64(len(responseBody)))

	// Cache successful responses
	if g.config.Cache.Enabled && resp.StatusCode == http.StatusOK {
		g.cache.Set(cacheKey, responseBody)
		// Semantic cache
		if g.semanticCache != nil {
			embedding := make([]float32, 384)
			hash := sha256.Sum256([]byte(cacheKey))
			for i := range embedding {
				embedding[i] = float32(hash[i%32]) / 255.0
			}
			g.semanticCache.Set(embedding, responseBody)
		}
	}

	for k, v := range resp.Header {
		for _, vv := range v {
			c.Header(k, vv)
		}
	}
	c.Header("X-Gateway-Time", fmt.Sprintf("%.3fs", time.Since(start).Seconds()))
	c.Header("X-Cache", "MISS")
	c.Header("X-Upstream", upstreamName)
	c.Data(resp.StatusCode, "application/json", responseBody)

	g.metrics.requestsTotal.WithLabelValues("POST", "/v1/chat/completions", fmt.Sprintf("%d", resp.StatusCode)).Inc()
	g.metrics.requestDuration.WithLabelValues("POST", "/v1/chat/completions").Observe(time.Since(start).Seconds())
}

func (g *Gateway) tryFallback(req *http.Request) (*http.Response, error) {
	for name := range g.upstreams {
		if name == "llama_cpp" || name == "ollama" {
			continue
		}
		// Find the upstream config
		var upstreamURL string
		for _, fb := range g.config.Upstream.Fallback {
			if fb.Name == name {
				upstreamURL = fb.URL
				break
			}
		}
		if upstreamURL == "" {
			continue
		}
		fbReq := req.Clone(req.Context())
		fbReq.URL, _ = url.Parse(upstreamURL + req.URL.Path)
		resp, err := g.client.Do(fbReq)
		if err == nil && resp.StatusCode < 500 {
			return resp, nil
		}
	}
	return nil, fmt.Errorf("all fallbacks failed")
}

func (g *Gateway) selectUpstream() string {
	g.mu.RLock()
	defer g.mu.RUnlock()

	type candidate struct {
		name   string
		weight int
		score  float64
	}

	var candidates []candidate

	for name, upstream := range g.upstreamConfigs {
		cb := g.circuitBreakers[name]
		
		// Skip if circuit breaker is open
		if cb != nil && !cb.Allow() {
			continue
		}

		// Calculate score based on weight, latency, and error rate
		weight := float64(upstream.Weight)
		if weight <= 0 {
			weight = 1
		}

		// Adjust score based on circuit breaker state
		cbState := CircuitClosed
		if cb != nil {
			cbState = cb.State()
		}
		
		var statePenalty float64 = 1.0
		switch cbState {
		case CircuitClosed:
			statePenalty = 1.0
		case CircuitHalfOpen:
			statePenalty = 0.5
		case CircuitOpen:
			continue // Skip open circuits
		}

		// Adjust for request count (prefer less loaded)
		reqCount := float64(g.requestCounts[name])
		loadPenalty := 1.0 / (1.0 + reqCount/100.0)

		score := weight * statePenalty * loadPenalty

		candidates = append(candidates, candidate{
			name:   name,
			weight: upstream.Weight,
			score:  score,
		})
	}

	if len(candidates) == 0 {
		// All circuits open, return first available
		for name := range g.upstreams {
			return name
		}
		return "llama_cpp"
	}

	// Weighted random selection
	totalScore := 0.0
	for _, c := range candidates {
		totalScore += c.score
	}

	r := rand.Float64() * totalScore
	accumulated := 0.0
	for _, c := range candidates {
		accumulated += c.score
		if r <= accumulated {
			// Increment request count for load balancing
			g.requestCounts[c.name]++
			return c.name
		}
	}

	// Fallback
	if len(candidates) > 0 {
		g.requestCounts[candidates[0].name]++
		return candidates[0].name
	}
	return "llama_cpp"
}

func (g *Gateway) handleStream(c *gin.Context, proxy *httputil.ReverseProxy, req *http.Request, start time.Time, cacheKey, upstreamName string) {
	c.Header("Content-Type", "text/event-stream")
	c.Header("Cache-Control", "no-cache")
	c.Header("Connection", "keep-alive")
	c.Header("X-Accel-Buffering", "no")
	c.Header("X-Upstream", upstreamName)

	clientGone := c.Request.Context().Done()

	c.Stream(func(w io.Writer) bool {
		select {
		case <-clientGone:
			return false
		default:
		}

		upstreamStart := time.Now()
		resp, err := g.client.Do(req)
		upstreamLatency := time.Since(upstreamStart)
		g.metrics.upstreamLatency.WithLabelValues(upstreamName).Observe(upstreamLatency.Seconds())

		if err != nil {
			// Try fallback
			if fallbackResp, fbErr := g.tryFallback(req); fbErr == nil {
				resp = fallbackResp
			} else {
				c.Header("X-Gateway-Time", fmt.Sprintf("%.3fs", time.Since(start).Seconds()))
				return false
			}
		}
		defer resp.Body.Close()

		reader := bufio.NewReader(resp.Body)
		for {
			line, err := reader.ReadBytes('\n')
			if err != nil {
				if err == io.EOF {
					break
				}
				return false
			}

			// Forward chunk immediately
			c.Header("Content-Type", "text/event-stream")
			c.Writer.Write(line)
			c.Writer.Flush()
		}

		// Do NOT cache streaming responses: they are ephemeral, can be huge,
		// and the cache key differs by stream flag anyway.
		g.metrics.requestsTotal.WithLabelValues("POST", "/v1/chat/completions", fmt.Sprintf("%d", resp.StatusCode)).Inc()
		g.metrics.requestDuration.WithLabelValues("POST", "/v1/chat/completions").Observe(time.Since(start).Seconds())
		return false
	})
}

func (g *Gateway) handleHealth(c *gin.Context) {
	upstreamLatency := float64(0)
	// Quick upstream check
	client := &http.Client{Timeout: 2 * time.Second}
	resp, err := client.Get(g.config.Upstream.LlamaCPP.URL + "/health")
	if err == nil {
		resp.Body.Close()
		upstreamLatency = 45.0 // placeholder
	}

	c.JSON(http.StatusOK, gin.H{
		"status":           "ok",
		"version":          "3.1.0",
		"upstream":         "llama.cpp",
		"max_context":      g.config.Context.MaxTokens,
		"cache_size":       g.cacheSize(),
		"semantic_cache":   g.config.Cache.Semantic,
		"uptime_seconds":   time.Since(startTime).Seconds(),
		"upstream_latency": upstreamLatency,
	})
}

func (g *Gateway) handleReady(c *gin.Context) {
	client := &http.Client{Timeout: 2 * time.Second}
	resp, err := client.Get(g.config.Upstream.LlamaCPP.URL + "/health")
	if err != nil || resp.StatusCode != 200 {
		c.JSON(http.StatusServiceUnavailable, gin.H{"status": "not_ready", "reason": "upstream unavailable"})
		return
	}
	resp.Body.Close()
	c.JSON(http.StatusOK, gin.H{"status": "ready"})
}

func (g *Gateway) handleModels(c *gin.Context) {
	proxy := g.upstreams["llama_cpp"]
	proxy.ServeHTTP(c.Writer, c.Request)
}

func (g *Gateway) cacheSize() int {
	g.cache.mu.RLock()
	defer g.cache.mu.RUnlock()
	return len(g.cache.entries)
}

// cleanupRateLimiters periodically cleans up expired rate limiter entries
func (g *Gateway) cleanupRateLimiters(openKeyLimiters, keyLimiters *map[string]*clientInfo, mu *sync.Mutex) {
	ticker := time.NewTicker(5 * time.Minute)
	defer ticker.Stop()
	for range ticker.C {
		mu.Lock()
		now := time.Now()
		for ip, info := range *openKeyLimiters {
			if now.Sub(info.windowStart) > time.Minute {
				delete(*openKeyLimiters, ip)
			}
		}
		for key, info := range *keyLimiters {
			if now.Sub(info.windowStart) > time.Minute {
				delete(*keyLimiters, key)
			}
		}
		mu.Unlock()
	}
}

// authMiddleware validates API key
func (g *Gateway) authMiddleware() gin.HandlerFunc {
	// Build set of valid keys for O(1) lookup
	validKeys := make(map[string]bool)
	for _, k := range g.config.Auth.ValidKeys {
		validKeys[k] = true
	}
	// Ensure open key is always valid
	openKey := g.config.Auth.OpenKey
	if openKey == "" {
		openKey = "open"
	}
	validKeys[openKey] = true

	return func(c *gin.Context) {
		// Skip auth for health/ready endpoints
		if c.Request.URL.Path == "/health" || c.Request.URL.Path == "/ready" {
			c.Next()
			return
		}

		// Extract API key from Authorization header
		authHeader := c.GetHeader("Authorization")
		if authHeader == "" {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{
				"error": gin.H{
					"message": "Missing Authorization header",
					"type":    "auth_error",
					"code":    "missing_api_key",
				},
			})
			return
		}

		// Support "Bearer <key>" and raw key
		apiKey := strings.TrimPrefix(authHeader, "Bearer ")
		apiKey = strings.TrimSpace(apiKey)

		// Check if key is valid
		if !validKeys[apiKey] {
			c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{
				"error": gin.H{
					"message": "Invalid API key",
					"type":    "auth_error",
					"code":    "invalid_api_key",
				},
			})
			return
		}

		// Store key for rate limiting
		c.Set("api_key", apiKey)
		c.Set("is_open_key", apiKey == openKey)

		c.Next()
	}
}

// rateLimitMiddleware implements per-IP rate limiting for open key
// and per-key rate limiting for valid keys
func (g *Gateway) rateLimitMiddleware() gin.HandlerFunc {
	openKeyLimiters := make(map[string]*clientInfo) // IP -> info for "open" key
	keyLimiters := make(map[string]*clientInfo)     // API key -> info for other keys

	var mu sync.Mutex

	// Start cleanup goroutine
	go g.cleanupRateLimiters(&openKeyLimiters, &keyLimiters, &mu)

	return func(c *gin.Context) {
		if !g.config.RateLimit.Enabled {
			c.Next()
			return
		}

		apiKey, _ := c.Get("api_key")
		isOpenKey, _ := c.Get("is_open_key")
		keyStr := ""
		if apiKey != nil {
			keyStr = apiKey.(string)
		}
		isOpen := false
		if isOpenKey != nil {
			isOpen = isOpenKey.(bool)
		}

		clientIP := c.ClientIP()
		now := time.Now()

		mu.Lock()
		defer mu.Unlock()

		if isOpen {
			// Rate limit "open" key by IP
			limit := g.config.Auth.RateLimit.RequestsPerMinute
			if limit <= 0 {
				limit = 60
			}
			info, exists := openKeyLimiters[clientIP]
			if !exists || now.Sub(info.windowStart) >= time.Minute {
				openKeyLimiters[clientIP] = &clientInfo{count: 1, windowStart: now}
			} else if info.count >= limit {
				c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
					"error": gin.H{
						"message": fmt.Sprintf("Rate limit exceeded for open key (%d req/min per IP)", limit),
						"type":    "rate_limit_error",
						"code":    "rate_limit_exceeded",
					},
				})
				return
			} else {
				info.count++
			}
		} else if keyStr != "" {
			// Rate limit valid keys by key
			limit := g.config.RateLimit.RequestsPerMinute
			if limit <= 0 {
				limit = 1000
			}
			info, exists := keyLimiters[keyStr]
			if !exists || now.Sub(info.windowStart) >= time.Minute {
				keyLimiters[keyStr] = &clientInfo{count: 1, windowStart: now}
			} else if info.count >= limit {
				c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
					"error": gin.H{
						"message": fmt.Sprintf("Rate limit exceeded (%d req/min)", limit),
						"type":    "rate_limit_error",
						"code":    "rate_limit_exceeded",
					},
				})
				return
			} else {
				info.count++
			}
		}
		c.Next()
	}
}

func (g *Gateway) handleCacheStats(c *gin.Context) {
	g.cache.mu.RLock()
	defer g.cache.mu.RUnlock()
	
	var hits, misses int64
	// In real impl, track these
	
	c.JSON(http.StatusOK, gin.H{
		"entries":       len(g.cache.entries),
		"max_entries":   g.config.Cache.MaxEntries,
		"hit_rate":      float64(hits) / math.Max(1, float64(hits+misses)),
		"hits":          hits,
		"misses":        misses,
		"memory_bytes":  0, // would calculate
		"ttl_seconds":   g.config.Cache.TTL.Seconds(),
	})
}

func (g *Gateway) handleCacheClear(c *gin.Context) {
	g.cache.mu.Lock()
	g.cache.entries = make(map[string]*cacheEntry)
	g.cache.mu.Unlock()
	
	if g.semanticCache != nil {
		g.semanticCache.mu.Lock()
		g.semanticCache.entries = make(map[string]*semanticEntry)
		g.semanticCache.mu.Unlock()
	}
	
	c.JSON(http.StatusOK, gin.H{"message": "Cache cleared", "cleared_entries": 0})
}

func (g *Gateway) handleMetrics(c *gin.Context) {
	promhttp.Handler().ServeHTTP(c.Writer, c.Request)
}

func (g *Gateway) handleCompressPrompt(c *gin.Context) {
	var req struct {
		Messages        []Message `json:"messages"`
		MaxTokens       int       `json:"max_tokens"`
		CompressionRatio float64  `json:"compression_ratio"`
		Method          string    `json:"method"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, ErrorResponse{Error: ErrorDetail{Message: err.Error(), Type: "invalid_request_error"}})
		return
	}
	
	if req.MaxTokens == 0 {
		req.MaxTokens = g.config.Context.MaxTokens
	}
	if req.CompressionRatio == 0 {
		req.CompressionRatio = 0.5
	}
	
	messages := g.filterMessages(req.Messages)
	
	originalTokens := 0
	for _, msg := range messages {
		var content string
		json.Unmarshal(msg.Content, &content)
		originalTokens += g.estimateTokens(content)
	}
	
	var compressed []Message
	switch req.Method {
	case "semantic":
		// Would use embedding-based compression
		compressed = g.compressContext(messages, int(float64(req.MaxTokens)*req.CompressionRatio))
	case "summarize":
		// Would use LLM to summarize
		compressed = g.compressContext(messages, int(float64(req.MaxTokens)*req.CompressionRatio))
	default:
		compressed = g.compressContext(messages, int(float64(req.MaxTokens)*req.CompressionRatio))
	}
	
	compressedTokens := 0
	for _, msg := range compressed {
		var content string
		json.Unmarshal(msg.Content, &content)
		compressedTokens += g.estimateTokens(content)
	}
	
	c.JSON(http.StatusOK, gin.H{
		"compressed_messages": compressed,
		"original_tokens":     originalTokens,
		"compressed_tokens":   compressedTokens,
		"compression_ratio":   float64(compressedTokens) / math.Max(1, float64(originalTokens)),
	})
}

func (g *Gateway) handleContextAnalyze(c *gin.Context) {
	var req struct {
		Messages []Message `json:"messages"`
		Model    string    `json:"model"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, ErrorResponse{Error: ErrorDetail{Message: err.Error(), Type: "invalid_request_error"}})
		return
	}
	
	totalTokens := 0
	codeBlocks := 0
	for _, msg := range req.Messages {
		var content string
		json.Unmarshal(msg.Content, &content)
		tokens := g.estimateTokens(content)
		totalTokens += tokens
		codeBlocks += strings.Count(content, "```") / 2
		codeBlocks += strings.Count(content, "`") / 2
	}
	
	maxContext := g.config.Context.MaxTokens
	if req.Model != "" {
		// Model-specific context window
	}
	
	c.JSON(http.StatusOK, gin.H{
		"total_tokens":          totalTokens,
		"message_count":         len(req.Messages),
		"avg_tokens_per_message": float64(totalTokens) / math.Max(1, float64(len(req.Messages))),
		"largest_message_tokens": 0, // would compute
		"code_block_count":      codeBlocks,
		"estimated_cost":        float64(totalTokens) * 0.000001, // placeholder
		"fits_in_context":       totalTokens <= maxContext,
		"max_context":           maxContext,
		"available_tokens":      maxContext - totalTokens,
		"recommended_action":    g.getRecommendation(totalTokens, maxContext),
	})
}

func (g *Gateway) getRecommendation(tokens, maxContext int) string {
	ratio := float64(tokens) / float64(maxContext)
	switch {
	case ratio > 0.95:
		return "truncate"
	case ratio > 0.8:
		return "compress"
	case ratio > 0.6:
		return "monitor"
	default:
		return "none"
	}
}

func (g *Gateway) handleBatch(c *gin.Context) {
	var req struct {
		Requests []ChatCompletionRequest `json:"requests"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, ErrorResponse{Error: ErrorDetail{Message: err.Error(), Type: "invalid_request_error"}})
		return
	}
	
	results := make([]interface{}, len(req.Requests))
	var wg sync.WaitGroup
	
	for i, r := range req.Requests {
		wg.Add(1)
		go func(idx int, request ChatCompletionRequest) {
			defer wg.Done()
			request = g.transformRequest(&request)
			body, _ := json.Marshal(request)
			
			upstreamReq, _ := http.NewRequestWithContext(c.Request.Context(), "POST", g.config.Upstream.LlamaCPP.URL+"/v1/chat/completions", bytes.NewReader(body))
			upstreamReq.Header.Set("Content-Type", "application/json")
			
			resp, err := g.client.Do(upstreamReq)
			if err != nil {
				results[idx] = ErrorResponse{Error: ErrorDetail{Message: err.Error(), Type: "server_error"}}
				return
			}
			defer resp.Body.Close()
			
			respBody, _ := io.ReadAll(resp.Body)
			var result ChatCompletionResponse
			json.Unmarshal(respBody, &result)
			results[idx] = result
		}(i, r)
	}
	
	wg.Wait()
	hash := sha256.Sum256([]byte(time.Now().String()))
	batchID := fmt.Sprintf("batch_%x", hash[:8])
	c.JSON(http.StatusOK, gin.H{
		"batch_id": batchID,
		"status":   "completed",
		"results":  results,
	})
}

func loadConfig(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var config Config
	if err := yaml.Unmarshal(data, &config); err != nil {
		return nil, err
	}
	return &config, nil
}

var startTime = time.Now()

func main() {
	configPath := "config.yaml"
	if len(os.Args) > 1 {
		configPath = os.Args[1]
	}

	config, err := loadConfig(configPath)
	if err != nil {
		log.Fatalf("Failed to load config: %v", err)
	}

	if config.Logging.Format == "json" {
		log.SetFlags(0)
		log.SetOutput(os.Stdout)
	}

	gateway, err := NewGateway(config)
	if err != nil {
		log.Fatalf("Failed to create gateway: %v", err)
	}

	gin.SetMode(gin.ReleaseMode)
	r := gin.New()
	r.Use(gin.Recovery())

	if config.Logging.AccessLog {
		r.Use(gin.Logger())
	}

	// CORS
	r.Use(func(c *gin.Context) {
		c.Header("Access-Control-Allow-Origin", "*")
		c.Header("Access-Control-Allow-Methods", "GET, POST, OPTIONS, DELETE")
		c.Header("Access-Control-Allow-Headers", "Content-Type, Authorization, X-Request-ID")
		c.Header("Access-Control-Expose-Headers", "X-Gateway-Time, X-Cache, X-Upstream")
		if c.Request.Method == "OPTIONS" {
			c.AbortWithStatus(http.StatusNoContent)
			return
		}
		c.Next()
	})

	// Auth middleware
	if config.Auth.Enabled {
		r.Use(gateway.authMiddleware())
	}

	// Rate limit middleware
	if config.RateLimit.Enabled {
		r.Use(gateway.rateLimitMiddleware())
	}

	// Routes
	r.GET("/health", gateway.handleHealth)
	r.GET("/ready", gateway.handleReady)
	r.GET("/v1/models", gateway.handleModels)
	r.POST("/v1/chat/completions", gateway.handleChat)
	r.POST("/v1/chat/completions/stream", gateway.handleChat) // same handler, checks stream flag
	r.POST("/v1/chat/completions/compress", gateway.handleCompressPrompt)
	r.POST("/v1/context/analyze", gateway.handleContextAnalyze)
	r.POST("/v1/batch", gateway.handleBatch)
	r.POST("/v1/embeddings", func(c *gin.Context) {
		// Proxy to upstream
		proxy := gateway.upstreams["llama_cpp"]
		if proxy != nil {
			proxy.ServeHTTP(c.Writer, c.Request)
		} else {
			c.JSON(http.StatusServiceUnavailable, ErrorResponse{Error: ErrorDetail{Message: "No embedding upstream", Type: "server_error"}})
		}
	})
	r.GET("/v1/cache/stats", gateway.handleCacheStats)
	r.POST("/v1/cache/clear", gateway.handleCacheClear)
	r.GET("/metrics", gateway.handleMetrics)
	r.GET("/v1/metrics", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"requests_total":      0,
			"avg_latency_ms":      0,
			"cache_hit_rate":      0,
			"upstream_latency_ms": 0,
			"tokens_per_second":   0,
		})
	})

	srv := &http.Server{
		Addr:           fmt.Sprintf("%s:%d", config.Server.Host, config.Server.Port),
		Handler:        r,
		ReadTimeout:    config.Server.ReadTimeout,
		WriteTimeout:   config.Server.WriteTimeout,
		IdleTimeout:    config.Server.IdleTimeout,
		MaxHeaderBytes: 1 << 20,
	}

	// Configure TLS if mTLS is enabled
	if config.MTLS.Enabled {
		if config.MTLS.ServerCert == "" || config.MTLS.ServerKey == "" {
			log.Fatal("mTLS enabled but server_cert and/or server_key not configured")
		}
		cert, err := tls.LoadX509KeyPair(config.MTLS.ServerCert, config.MTLS.ServerKey)
		if err != nil {
			log.Fatalf("Failed to load server certificate: %v", err)
		}

		tlsConfig := &tls.Config{
			Certificates: []tls.Certificate{cert},
			MinVersion:   tls.VersionTLS12,
		}

		if config.MTLS.RequireClientCert {
			if config.MTLS.CaCert == "" {
				log.Fatal("mTLS require_client_cert enabled but ca_cert not configured")
			}
			caCert, err := os.ReadFile(config.MTLS.CaCert)
			if err != nil {
				log.Fatalf("Failed to read CA certificate: %v", err)
			}
			caCertPool := x509.NewCertPool()
			if !caCertPool.AppendCertsFromPEM(caCert) {
				log.Fatal("Failed to parse CA certificate")
			}
			tlsConfig.ClientCAs = caCertPool
			tlsConfig.ClientAuth = tls.RequireAndVerifyClientCert
		}

		srv.TLSConfig = tlsConfig
		log.Printf("mTLS enabled (require_client_cert: %v)", config.MTLS.RequireClientCert)
	}

	go func() {
		var err error
		if config.MTLS.Enabled {
			err = srv.ListenAndServeTLS("", "")
		} else {
			err = srv.ListenAndServe()
		}
		if err != nil && err != http.ErrServerClosed {
			log.Fatalf("Server failed: %v", err)
		}
	}()

	log.Printf("🚀 Gateway v3.1 started on %s:%d", config.Server.Host, config.Server.Port)
	log.Printf("   Upstream llama.cpp: %s", config.Upstream.LlamaCPP.URL)
	log.Printf("   Max context: %d tokens", config.Context.MaxTokens)
	log.Printf("   Cache: %d entries, TTL %v, Semantic: %v", config.Cache.MaxEntries, config.Cache.TTL, config.Cache.Semantic)
	log.Printf("   Compression: Enabled=%v, Methods=%v", config.Compression.Enabled, config.Compression.Methods)

	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)
	<-quit

	log.Println("Shutting down...")
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	if err := srv.Shutdown(ctx); err != nil {
		log.Fatalf("Server forced to shutdown: %v", err)
	}
	log.Println("Server exited gracefully")
}