package main

// Test file for standalone Go file without go.mod

type Config struct {
	Host string
	Port int
}

// mantra: 設定を検証
func ValidateConfig(cfg *Config) error {
	panic("not implemented")
}