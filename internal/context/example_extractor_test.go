package context

import (
	"strings"
	"testing"

	"github.com/rail44/glyph/internal/parser"
)

func TestExampleExtractor_ExtractFromFileContent(t *testing.T) {
	fileContent := `
package main

import (
	"fmt"
	"errors"
)

type User struct {
	ID   string
	Name string
}

// GetUserByID retrieves a user by ID
func GetUserByID(id string) (*User, error) {
	if id == "" {
		return nil, errors.New("id is required")
	}
	// Simulated implementation
	return &User{ID: id, Name: "John Doe"}, nil
}

// glyph: emailでユーザーを検索
func GetUserByEmail(email string) (*User, error) {
	panic("not implemented")
}

// ProcessUser processes a user
func ProcessUser(user *User) error {
	if user == nil {
		return errors.New("user is nil")
	}
	fmt.Printf("Processing user: %s\n", user.Name)
	return nil
}

// NotImplementedFunc is not implemented
func NotImplementedFunc() {
	panic("not implemented")
}

// EmptyFunc has no body
func EmptyFunc()
`

	target := &parser.Target{
		Name: "GetUserByEmail",
		Params: []parser.Param{
			{Name: "email", Type: "string"},
		},
		Returns: []parser.Return{
			{Type: "*User"},
			{Type: "error"},
		},
	}

	extractor := NewExampleExtractor()
	examples, err := extractor.ExtractFromFileContent(fileContent, target)
	if err != nil {
		t.Fatalf("Failed to extract examples: %v", err)
	}

	// Should extract GetUserByID and ProcessUser, but not GetUserByEmail (target), NotImplementedFunc (has panic), or EmptyFunc (no body)
	expectedCount := 2
	if len(examples) != expectedCount {
		t.Errorf("Expected %d examples, got %d", expectedCount, len(examples))
	}

	// Check first example (GetUserByID)
	if len(examples) > 0 {
		ex := examples[0]
		if ex.Name != "GetUserByID" {
			t.Errorf("Expected first example name to be GetUserByID, got %s", ex.Name)
		}
		if !strings.Contains(ex.Signature, "func GetUserByID(id string) (*User, error)") {
			t.Errorf("Unexpected signature: %s", ex.Signature)
		}
		if !strings.Contains(ex.Body, `if id == ""`) {
			t.Errorf("Expected body to contain id check, got: %s", ex.Body)
		}
		if len(ex.Params) != 1 || ex.Params[0].Name != "id" || ex.Params[0].Type != "string" {
			t.Errorf("Unexpected params: %+v", ex.Params)
		}
		if len(ex.Returns) != 2 || ex.Returns[0].Type != "*User" || ex.Returns[1].Type != "error" {
			t.Errorf("Unexpected returns: %+v", ex.Returns)
		}
	}

	// Check second example (ProcessUser)
	if len(examples) > 1 {
		ex := examples[1]
		if ex.Name != "ProcessUser" {
			t.Errorf("Expected second example name to be ProcessUser, got %s", ex.Name)
		}
		if !strings.Contains(ex.Signature, "func ProcessUser(user *User) error") {
			t.Errorf("Unexpected signature: %s", ex.Signature)
		}
	}
}

func TestExampleExtractor_WithMethods(t *testing.T) {
	fileContent := `
package service

type UserService struct {
	db Database
}

// glyph: IDでユーザーを取得
func (s *UserService) GetByID(id string) (*User, error) {
	panic("not implemented")
}

// GetAll retrieves all users
func (s *UserService) GetAll() ([]*User, error) {
	users, err := s.db.Query("SELECT * FROM users")
	if err != nil {
		return nil, err
	}
	return users, nil
}

// Delete deletes a user
func (s UserService) Delete(id string) error {
	return s.db.Delete("users", id)
}
`

	target := &parser.Target{
		Name: "GetByID",
		Receiver: &parser.Receiver{
			Name: "s",
			Type: "*UserService",
		},
	}

	extractor := NewExampleExtractor()
	examples, err := extractor.ExtractFromFileContent(fileContent, target)
	if err != nil {
		t.Fatalf("Failed to extract examples: %v", err)
	}

	// Should extract GetAll and Delete
	if len(examples) != 2 {
		t.Errorf("Expected 2 examples, got %d", len(examples))
	}

	// Check method receivers
	for _, ex := range examples {
		if ex.Receiver == nil {
			t.Errorf("Expected example %s to have receiver", ex.Name)
			continue
		}
		if ex.Name == "GetAll" && ex.Receiver.Type != "*UserService" {
			t.Errorf("Expected GetAll receiver type to be *UserService, got %s", ex.Receiver.Type)
		}
		if ex.Name == "Delete" && ex.Receiver.Type != "UserService" {
			t.Errorf("Expected Delete receiver type to be UserService, got %s", ex.Receiver.Type)
		}
	}
}

func TestExampleExtractor_EmptyFile(t *testing.T) {
	fileContent := `package main`

	target := &parser.Target{
		Name: "SomeFunc",
	}

	extractor := NewExampleExtractor()
	examples, err := extractor.ExtractFromFileContent(fileContent, target)
	if err != nil {
		t.Fatalf("Failed to extract examples: %v", err)
	}

	if len(examples) != 0 {
		t.Errorf("Expected 0 examples from empty file, got %d", len(examples))
	}
}

func TestExampleExtractor_InvalidGo(t *testing.T) {
	fileContent := `This is not valid Go code`

	target := &parser.Target{
		Name: "SomeFunc",
	}

	extractor := NewExampleExtractor()
	_, err := extractor.ExtractFromFileContent(fileContent, target)
	if err == nil {
		t.Error("Expected error for invalid Go code, got nil")
	}
}