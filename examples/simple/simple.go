package test_simple

import (
	"sync"
	"time"
)

type cacheItem struct {
	value     interface{}
	expiresAt time.Time
}

// SimpleCache is a simple cache interface for testing
type SimpleCache struct {
	mu    sync.RWMutex
	items map[string]cacheItem
}

// mantra: キャッシュから値を取得する。存在しない場合はnilを返す。有効期限が切れている場合もnilを返す。RLockを使用すること
func (c *SimpleCache) Get(key string) interface{} {
	panic("not implemented")
}

// mantra: キャッシュに値を設定する。TTLが0の場合は有効期限なし。Lockを使用すること。itemsがnilの場合は初期化すること
func (c *SimpleCache) Set(key string, value interface{}, ttl time.Duration) {
	panic("not implemented")
}

// mantra: キャッシュから指定されたキーを削除する
func (c *SimpleCache) Delete(key string) {
	panic("not implemented")
}
