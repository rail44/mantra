package examples

import (
	"context"
	"fmt"
	"time"

	"cloud.google.com/go/spanner"
	"google.golang.org/api/iterator"
)

// ExecuteGetUserRequest retrieves user details by their unique ID from Spanner
func ExecuteGetUserRequest(ctx context.Context, client *spanner.Client, req *GetUserRequest) (*GetUserResponse, error) {
	if req.UserID == "" {
		return nil, fmt.Errorf("user_id is required")
	}

	// Use read-only transaction for better performance
	txn := client.ReadOnlyTransaction()
	defer txn.Close()

	query := `SELECT id, email, name, created_at, updated_at FROM users WHERE id = @id`
	stmt := spanner.Statement{
		SQL: query,
		Params: map[string]interface{}{
			"id": req.UserID,
		},
	}

	var resp *GetUserResponse
	iter := txn.Query(ctx, stmt)
	defer iter.Stop()

	row, err := iter.Next()
	if err == iterator.Done {
		return nil, fmt.Errorf("user with ID %s not found", req.UserID)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to query user: %w", err)
	}

	var id, email, name string
	var createdAt, updatedAt time.Time

	if err := row.Columns(&id, &email, &name, &createdAt, &updatedAt); err != nil {
		return nil, fmt.Errorf("failed to scan user data: %w", err)
	}

	resp = &GetUserResponse{
		ID:        id,
		Email:     email,
		Name:      name,
		CreatedAt: createdAt,
		UpdatedAt: updatedAt,
	}

	return resp, nil
}
