package modelfile

// Built-in mode configurations
var builtinModes = map[string]ModeConfig{
	"spanner": {
		Name:        "spanner",
		Description: "Google Cloud Spanner optimized generation",
		BaseModel:   "devstral",
		Parameters: map[string]interface{}{
			"temperature": 0.3,
			"top_p":       0.9,
		},
		Principles: []string{
			"Use ReadOnlyTransaction for read operations",
			"Handle iterator.Done properly in query loops",
			"Include context in all error messages using fmt.Errorf",
			"Use parameterized queries to prevent SQL injection",
			"Prefer batch operations for better performance",
		},
		Patterns: []Pattern{
			{
				Name:        "Query Pattern",
				Description: "Standard query execution with iterator",
				Example: `txn := client.ReadOnlyTransaction()
defer txn.Close()

iter := txn.Query(ctx, stmt)
defer iter.Stop()

for {
    row, err := iter.Next()
    if err == iterator.Done {
        break
    }
    if err != nil {
        return nil, fmt.Errorf("failed to iterate: %w", err)
    }
    // Process row
}`,
			},
			{
				Name:        "Single Row Read",
				Description: "Reading a single row by key",
				Example: `row, err := client.Single().ReadRow(ctx, "table", spanner.Key{id}, columns)
if err != nil {
    if err == spanner.ErrRowNotFound {
        return nil, fmt.Errorf("not found: %w", err)
    }
    return nil, fmt.Errorf("failed to read: %w", err)
}`,
			},
		},
	},
	"generic": {
		Name:        "generic",
		Description: "Generic Go code generation",
		BaseModel:   "devstral",
		Parameters: map[string]interface{}{
			"temperature": 0.5,
		},
		Principles: []string{
			"Follow Go idioms and best practices",
			"Use proper error handling",
			"Write clear and maintainable code",
			"Include appropriate documentation",
		},
		Patterns: []Pattern{},
	},
}

// Built-in system prompts
var builtinSystemPrompts = map[string]string{
	"spanner": `You are an expert Go developer specializing in Google Cloud Spanner.
You have deep knowledge of Spanner's distributed architecture and best practices.
Focus on generating efficient, scalable code that properly handles:
- Distributed transactions
- Read/write splits
- Query optimization
- Error handling with proper context
- Connection management

Always use the patterns and principles provided in the context.`,

	"generic": `You are an expert Go developer who writes clean, idiomatic Go code.
Focus on:
- Clear and maintainable code structure
- Proper error handling
- Following Go conventions
- Efficient algorithms and data structures`,
}