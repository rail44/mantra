package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"os"

	"github.com/rail44/mantra/internal/tools"
	"github.com/rail44/mantra/internal/tools/impl"
	"github.com/rail44/mantra/internal/tools/setup"
)

func main() {
	// Get project root (current directory for testing)
	projectRoot, err := os.Getwd()
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Testing Mantra Tools in: %s\n\n", projectRoot)

	// Create tool instances
	inspectTool := impl.NewInspectTool()
	searchTool := impl.NewSearchTool(projectRoot)
	readBodyTool := impl.NewReadBodyTool(projectRoot)
	checkSyntaxTool := impl.NewCheckSyntaxTool()

	ctx := context.Background()

	// Test 1: Search for Repository types
	fmt.Println("=== Test 1: Search for Repository types ===")
	searchParams := map[string]interface{}{
		"pattern": "*Repository",
		"kind":    "interface",
		"limit":   5,
	}
	result, err := searchTool.Execute(ctx, searchParams)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	} else {
		printJSON(result)
	}

	// Test 2: Check syntax of valid code
	fmt.Println("\n=== Test 2: Check syntax (valid code) ===")
	validCode := `user, err := s.repo.GetByEmail(ctx, email)
if err != nil {
    return nil, err
}
return user, nil`
	
	syntaxParams := map[string]interface{}{
		"code":    validCode,
		"context": "function_body",
	}
	result, err = checkSyntaxTool.Execute(ctx, syntaxParams)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	} else {
		printJSON(result)
	}

	// Test 3: Check syntax of invalid code
	fmt.Println("\n=== Test 3: Check syntax (invalid code) ===")
	invalidCode := `if user != nil {
    return nil, fmt.Errorf("user exists"
}`
	
	syntaxParams = map[string]interface{}{
		"code":    invalidCode,
		"context": "function_body",
	}
	result, err = checkSyntaxTool.Execute(ctx, syntaxParams)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	} else {
		printJSON(result)
	}

	// Test 4: Search for functions
	fmt.Println("\n=== Test 4: Search for Create* methods ===")
	searchParams = map[string]interface{}{
		"pattern": "Create*",
		"kind":    "method",
		"limit":   5,
	}
	result, err = searchTool.Execute(ctx, searchParams)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	} else {
		printJSON(result)
	}

	// Test 5: Inspect a type
	fmt.Println("\n=== Test 5: Inspect UserRepository interface ===")
	inspectParams := map[string]interface{}{
		"name": "UserRepository",
	}
	result, err = inspectTool.Execute(ctx, inspectParams)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	} else {
		printJSON(result)
	}

	// Test 6: Inspect a function
	fmt.Println("\n=== Test 6: Inspect main function ===")
	inspectParams = map[string]interface{}{
		"name": "main",
	}
	result, err = inspectTool.Execute(ctx, inspectParams)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	} else {
		printJSON(result)
	}

	// Test 7: Read body of a function
	fmt.Println("\n=== Test 7: Read body of CreateUser method ===")
	readParams := map[string]interface{}{
		"name": "CreateUser",
	}
	result, err = readBodyTool.Execute(ctx, readParams)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	} else {
		printJSON(result)
	}

	// Test 8: Test tool registry
	fmt.Println("\n=== Test 8: Tool Registry ===")
	registry := setup.InitializeRegistry(projectRoot)
	available := registry.ListAvailable()
	fmt.Printf("Available tools: %d\n", len(available))
	for _, tool := range available {
		fmt.Printf("- %s: %s\n", tool.Name, tool.Description)
	}

	// Test 9: Test executor
	fmt.Println("\n=== Test 9: Tool Executor ===")
	executor := tools.NewExecutor(registry)
	result, err = executor.Execute(ctx, "check_syntax", syntaxParams)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
	} else {
		fmt.Println("Executor test passed!")
		printJSON(result)
	}
}

func printJSON(v interface{}) {
	b, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		fmt.Printf("Error marshaling JSON: %v\n", err)
		return
	}
	fmt.Println(string(b))
}