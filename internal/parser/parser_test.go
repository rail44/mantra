package parser

import (
	"os"
	"path/filepath"
	"testing"
)

func TestParseFile(t *testing.T) {
	// Create a temporary test file
	tmpDir := t.TempDir()
	testFile := filepath.Join(tmpDir, "test.go")
	
	content := `package test

// UserRequest represents a user query
// @description Get user information by ID
type UserRequest struct {
	UserID string ` + "`json:\"user_id\"`" + ` // The user's unique identifier
}

// UserResponse represents the response
type UserResponse struct {
	ID    string ` + "`json:\"id\"`" + `
	Name  string ` + "`json:\"name\"`" + `
	Email string ` + "`json:\"email\"`" + `
}
`
	
	err := os.WriteFile(testFile, []byte(content), 0644)
	if err != nil {
		t.Fatalf("Failed to create test file: %v", err)
	}

	// Parse the file
	decl, err := ParseFile(testFile)
	if err != nil {
		t.Fatalf("Failed to parse file: %v", err)
	}

	// Verify results
	if decl.RequestType != "UserRequest" {
		t.Errorf("Expected RequestType 'UserRequest', got '%s'", decl.RequestType)
	}

	if decl.ResponseType != "UserResponse" {
		t.Errorf("Expected ResponseType 'UserResponse', got '%s'", decl.ResponseType)
	}

	if decl.Description != "Get user information by ID" {
		t.Errorf("Expected description 'Get user information by ID', got '%s'", decl.Description)
	}

	if len(decl.Fields) != 1 {
		t.Fatalf("Expected 1 field, got %d", len(decl.Fields))
	}

	field := decl.Fields[0]
	if field.Name != "UserID" {
		t.Errorf("Expected field name 'UserID', got '%s'", field.Name)
	}

	if field.Type != "string" {
		t.Errorf("Expected field type 'string', got '%s'", field.Type)
	}
}