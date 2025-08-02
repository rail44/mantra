package context

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/rail44/mantra/internal/parser"
)

func TestExtractProjectContext(t *testing.T) {
	tests := []struct {
		name          string
		setup         func(t *testing.T) (string, func())
		targetName    string
		wantPackage   string
		wantTypeCount int
		wantErr       bool
	}{
		{
			name: "standalone_file_without_go_mod",
			setup: func(t *testing.T) (string, func()) {
				// Create a temporary directory
				tmpDir := t.TempDir()

				// Create a standalone Go file
				content := `package main

import "fmt"

type User struct {
	ID   string
	Name string
}

// mantra: ユーザーを作成
func CreateUser(name string) (*User, error) {
	panic("not implemented")
}

func main() {
	fmt.Println("test")
}
`
				filePath := filepath.Join(tmpDir, "standalone.go")
				err := os.WriteFile(filePath, []byte(content), 0644)
				if err != nil {
					t.Fatal(err)
				}

				return filePath, func() {}
			},
			targetName:    "CreateUser",
			wantPackage:   "main",
			wantTypeCount: 1, // User struct
			wantErr:       false,
		},
		{
			name: "file_in_go_module",
			setup: func(t *testing.T) (string, func()) {
				// Create a temporary directory
				tmpDir := t.TempDir()

				// Create go.mod
				goModContent := `module testmodule

go 1.21
`
				err := os.WriteFile(filepath.Join(tmpDir, "go.mod"), []byte(goModContent), 0644)
				if err != nil {
					t.Fatal(err)
				}

				// Create a Go file
				content := `package mypackage

type Config struct {
	Host string
	Port int
}

type Service struct {
	config *Config
}

// mantra: サービスを初期化
func NewService(cfg *Config) *Service {
	panic("not implemented")
}
`
				filePath := filepath.Join(tmpDir, "service.go")
				err = os.WriteFile(filePath, []byte(content), 0644)
				if err != nil {
					t.Fatal(err)
				}

				return filePath, func() {}
			},
			targetName:    "NewService",
			wantPackage:   "mypackage",
			wantTypeCount: 2, // Config and Service
			wantErr:       false,
		},
		{
			name: "file_in_subdirectory_of_module",
			setup: func(t *testing.T) (string, func()) {
				// Create a temporary directory
				tmpDir := t.TempDir()

				// Create go.mod
				goModContent := `module github.com/test/project

go 1.21
`
				err := os.WriteFile(filepath.Join(tmpDir, "go.mod"), []byte(goModContent), 0644)
				if err != nil {
					t.Fatal(err)
				}

				// Create subdirectory
				subDir := filepath.Join(tmpDir, "internal", "handler")
				err = os.MkdirAll(subDir, 0755)
				if err != nil {
					t.Fatal(err)
				}

				// Create a Go file in subdirectory
				content := `package handler

import "context"

type Request struct {
	ID string
}

type Response struct {
	Message string
}

// mantra: リクエストを処理
func HandleRequest(ctx context.Context, req *Request) (*Response, error) {
	panic("not implemented")
}
`
				filePath := filepath.Join(subDir, "handler.go")
				err = os.WriteFile(filePath, []byte(content), 0644)
				if err != nil {
					t.Fatal(err)
				}

				return filePath, func() {}
			},
			targetName:    "HandleRequest",
			wantPackage:   "handler",
			wantTypeCount: 2, // Request and Response
			wantErr:       false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test environment
			filePath, cleanup := tt.setup(t)
			defer cleanup()

			// Parse the file to get the target
			fileInfo, err := parser.ParseFileInfo(filePath)
			if err != nil {
				t.Fatalf("failed to parse file: %v", err)
			}

			// Find the target function
			var target *parser.Target
			for _, t := range fileInfo.Targets {
				if t.Name == tt.targetName {
					target = t
					break
				}
			}
			if target == nil {
				t.Fatalf("target function %s not found", tt.targetName)
			}

			// Test ExtractProjectContext
			ctx, err := ExtractProjectContext(filePath, target)

			// Check error
			if (err != nil) != tt.wantErr {
				t.Errorf("ExtractProjectContext() error = %v, wantErr %v", err, tt.wantErr)
				return
			}

			if tt.wantErr {
				return
			}

			// Check package name
			if ctx.PackageName != tt.wantPackage {
				t.Errorf("PackageName = %v, want %v", ctx.PackageName, tt.wantPackage)
			}

			// Check extracted types
			if len(ctx.Types) != tt.wantTypeCount {
				t.Errorf("Type count = %v, want %v", len(ctx.Types), tt.wantTypeCount)
				t.Logf("Extracted types: %v", ctx.Types)
			}

			// Log results if test fails or verbose flag is set
			if t.Failed() || testing.Verbose() {
				t.Logf("Test case: %s", tt.name)
				t.Logf("File path: %s", filePath)
				t.Logf("Package: %s", ctx.PackageName)
				t.Logf("Types found: %d (expected: %d)", len(ctx.Types), tt.wantTypeCount)
				for name, def := range ctx.Types {
					t.Logf("  - %s: %s", name, def)
				}
			}
		})
	}
}
