// Package cache provides a thread-safe in-memory cache implementation.
//
// It supports TTL-based expiration and automatic cleanup of expired entries.
//
// Example usage:
//
//	c := cache.New(5 * time.Minute)
//	c.Set("key", "value")
//	value, ok := c.Get("key")
package cache

import (
	"sync"
	"time"
)

// Cache represents an in-memory key-value store with expiration.
// It is safe for concurrent use by multiple goroutines.
//
// Cache uses a read-write mutex to ensure thread safety while
// maintaining good read performance.
type Cache struct {
	mu      sync.RWMutex
	items   map[string]item
	ttl     time.Duration
	cleanup *time.Ticker
}

// item represents a cached value with its expiration time.
type item struct {
	value      interface{}
	expiration time.Time
}

// New creates a new Cache with the specified default TTL.
// It starts a background goroutine to periodically clean up expired items.
//
// The cleanup interval is set to half the TTL duration.
//
// Example:
//
//	cache := New(10 * time.Minute)
//	defer cache.Close()
func New(ttl time.Duration) *Cache {
	c := &Cache{
		items:   make(map[string]item),
		ttl:     ttl,
		cleanup: time.NewTicker(ttl / 2),
	}
	go c.cleanupLoop()
	return c
}

// Get retrieves a value from the cache.
// It returns the value and true if found and not expired,
// or nil and false otherwise.
//
// Get is safe for concurrent use.
func (c *Cache) Get(key string) (interface{}, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()

	item, found := c.items[key]
	if !found {
		return nil, false
	}

	if time.Now().After(item.expiration) {
		return nil, false
	}

	return item.value, true
}

// Set stores a value in the cache with the default TTL.
// If the key already exists, its value and expiration are updated.
//
// Set is safe for concurrent use.
func (c *Cache) Set(key string, value interface{}) {
	c.SetWithTTL(key, value, c.ttl)
}

// SetWithTTL stores a value with a custom TTL.
// This allows different expiration times for different keys.
//
// BUG(alice): SetWithTTL does not validate that ttl is positive.
func (c *Cache) SetWithTTL(key string, value interface{}, ttl time.Duration) {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.items[key] = item{
		value:      value,
		expiration: time.Now().Add(ttl),
	}
}

// Delete removes an item from the cache.
// It returns true if the item existed, false otherwise.
func (c *Cache) Delete(key string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()

	_, found := c.items[key]
	if found {
		delete(c.items, key)
	}
	return found
}

// Clear removes all items from the cache.
func (c *Cache) Clear() {
	c.mu.Lock()
	defer c.mu.Unlock()

	c.items = make(map[string]item)
}

// Len returns the number of items in the cache.
// Note that this includes expired items that haven't been cleaned up yet.
//
// Deprecated: Use Count instead which excludes expired items.
func (c *Cache) Len() int {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return len(c.items)
}

// Count returns the number of non-expired items.
// It is more accurate than Len but slower.
//
// See also: Len, Clear
func (c *Cache) Count() int {
	c.mu.RLock()
	defer c.mu.RUnlock()

	count := 0
	now := time.Now()
	for _, item := range c.items {
		if now.Before(item.expiration) {
			count++
		}
	}
	return count
}

// Close stops the cleanup goroutine and releases resources.
// After Close is called, the cache should not be used.
//
// TODO: Add context support for graceful shutdown.
func (c *Cache) Close() {
	c.cleanup.Stop()
}

// cleanupLoop periodically removes expired items.
func (c *Cache) cleanupLoop() {
	for range c.cleanup.C {
		c.removeExpired()
	}
}

// removeExpired deletes all expired items.
func (c *Cache) removeExpired() {
	c.mu.Lock()
	defer c.mu.Unlock()

	now := time.Now()
	for key, item := range c.items {
		if now.After(item.expiration) {
			delete(c.items, key)
		}
	}
}
