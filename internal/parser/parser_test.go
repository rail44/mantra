package parser

import (
	"os"
	"path/filepath"
	"testing"
)

func TestParseFile(t *testing.T) {
	// Create a temporary test file
	tempDir := t.TempDir()
	testFile := filepath.Join(tempDir, "test.go")

	testContent := `package test

import (
	"context"
	"time"
)

type User struct {
	ID    string
	Email string
	Name  string
}

// mantra: emailでユーザーを検索
func GetUserByEmail(ctx context.Context, email string) (*User, error) {
	panic("not implemented")
}

// mantra: 複数のユーザーを取得
// statusがactiveのユーザーのみ
// created_atの降順でソート
func ListActiveUsers(ctx context.Context, limit int) ([]*User, error) {
	panic("not implemented")
}

// This function has no mantra comment
func IgnoredFunction() {
	// Do nothing
}

type Repository struct {
	client any
}

// mantra: ユーザーをIDで取得
func (r *Repository) GetUser(ctx context.Context, id string) (*User, error) {
	panic("not implemented")
}

// mantra: 新規ユーザーを作成
func (r *Repository) CreateUser(ctx context.Context, user *User) error {
	panic("not implemented")
}

// mantra: 割引率を計算
func CalculateDiscount(price float64) float64 {
	panic("not implemented")
}
`

	err := os.WriteFile(testFile, []byte(testContent), 0644)
	if err != nil {
		t.Fatalf("Failed to write test file: %v", err)
	}

	// Parse the file
	targets, err := ParseFile(testFile)
	if err != nil {
		t.Fatalf("ParseFile failed: %v", err)
	}

	// Check number of targets
	if len(targets) != 5 {
		t.Errorf("Expected 5 targets, got %d", len(targets))
	}

	// Test cases
	tests := []struct {
		name           string
		expectedName   string
		expectedInstr  string
		expectedParams int
		expectedRets   int
		hasReceiver    bool
		hasPanic       bool
	}{
		{
			name:           "GetUserByEmail",
			expectedName:   "GetUserByEmail",
			expectedInstr:  "emailでユーザーを検索",
			expectedParams: 2,
			expectedRets:   2,
			hasReceiver:    false,
			hasPanic:       true,
		},
		{
			name:           "ListActiveUsers",
			expectedName:   "ListActiveUsers",
			expectedInstr:  "複数のユーザーを取得\nstatusがactiveのユーザーのみ\ncreated_atの降順でソート",
			expectedParams: 2,
			expectedRets:   2,
			hasReceiver:    false,
			hasPanic:       true,
		},
		{
			name:           "Repository.GetUser",
			expectedName:   "GetUser",
			expectedInstr:  "ユーザーをIDで取得",
			expectedParams: 2,
			expectedRets:   2,
			hasReceiver:    true,
			hasPanic:       true,
		},
		{
			name:           "Repository.CreateUser",
			expectedName:   "CreateUser",
			expectedInstr:  "新規ユーザーを作成",
			expectedParams: 2,
			expectedRets:   1,
			hasReceiver:    true,
			hasPanic:       true,
		},
		{
			name:           "CalculateDiscount",
			expectedName:   "CalculateDiscount",
			expectedInstr:  "割引率を計算",
			expectedParams: 1,
			expectedRets:   1,
			hasReceiver:    false,
			hasPanic:       true,
		},
	}

	// Find and verify each target
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var target *Target
			for _, tgt := range targets {
				if tgt.Name == tt.expectedName {
					target = tgt
					break
				}
			}

			if target == nil {
				t.Fatalf("Target %s not found", tt.expectedName)
			}

			if target.Instruction != tt.expectedInstr {
				t.Errorf("Expected instruction %q, got %q", tt.expectedInstr, target.Instruction)
			}

			if len(target.Params) != tt.expectedParams {
				t.Errorf("Expected %d params, got %d", tt.expectedParams, len(target.Params))
			}

			if len(target.Returns) != tt.expectedRets {
				t.Errorf("Expected %d returns, got %d", tt.expectedRets, len(target.Returns))
			}

			if (target.Receiver != nil) != tt.hasReceiver {
				t.Errorf("Expected hasReceiver=%v, got %v", tt.hasReceiver, target.Receiver != nil)
			}

			if target.HasPanic != tt.hasPanic {
				t.Errorf("Expected hasPanic=%v, got %v", tt.hasPanic, target.HasPanic)
			}
		})
	}
}

func TestGetFunctionSignature(t *testing.T) {
	tests := []struct {
		name     string
		target   Target
		expected string
	}{
		{
			name: "Simple function",
			target: Target{
				Name: "GetUser",
				Params: []Param{
					{Name: "id", Type: "string"},
				},
				Returns: []Return{
					{Type: "*User"},
					{Type: "error"},
				},
			},
			expected: "func GetUser(id string) (*User, error)",
		},
		{
			name: "Method with receiver",
			target: Target{
				Name: "CreateUser",
				Receiver: &Receiver{
					Name: "r",
					Type: "*Repository",
				},
				Params: []Param{
					{Name: "ctx", Type: "context.Context"},
					{Name: "user", Type: "*User"},
				},
				Returns: []Return{
					{Type: "error"},
				},
			},
			expected: "func (r *Repository) CreateUser(ctx context.Context, user *User) error",
		},
		{
			name: "Function with no params",
			target: Target{
				Name:    "GetVersion",
				Returns: []Return{{Type: "string"}},
			},
			expected: "func GetVersion() string",
		},
		{
			name: "Function with no returns",
			target: Target{
				Name: "LogMessage",
				Params: []Param{
					{Name: "msg", Type: "string"},
				},
			},
			expected: "func LogMessage(msg string)",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			sig := tt.target.GetFunctionSignature()
			if sig != tt.expected {
				t.Errorf("Expected %q, got %q", tt.expected, sig)
			}
		})
	}
}
