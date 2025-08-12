package test_simple

import (
	"time"
)

type cacheItem struct {
	value     any
	expiresAt time.Time
}

// SimpleCache is a simple cache interface for testing
type SimpleCache struct {
	items map[string]cacheItem
}

// mantra: キャッシュから値を取得する。存在しない場合はnilを返す。有効期限が切れている場合もnilを返す。
func (c *SimpleCache) Get(key string) any {
	panic("not implemented")
}

// mantra: キャッシュに値を設定する。TTLが0の場合は有効期限なし。
func (c *SimpleCache) Set(key string, value any, ttl time.Duration) {
	panic("not implemented")
}

// mantra: キャッシュから指定されたキーを削除する
func (c *SimpleCache) Delete(key string) {
	panic("not implemented")
}

// mantra: キャッシュを全てクリアする。
func (c *SimpleCache) Clear() {
	panic("not implemented")
}

// mantra: キャッシュのサイズを返す。
func (c *SimpleCache) Size() int {
	panic("not implemented")
}

// mantra: 全てのキーを返す
func (c *SimpleCache) Keys() []string {
	panic("not implemented")
}

// mantra: オポポースをオポポースしとく
func Opoporse() string {
	panic("not implemented")
}
