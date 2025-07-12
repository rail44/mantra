package examples

import (
	"context"
	"fmt"
	"time"

	"cloud.google.com/go/spanner"
	"google.golang.org/api/iterator"
)

func ExecuteListUsersRequest(ctx context.Context, client *spanner.Client, req *ListUsersRequest) (*ListUsersResponse, error) {
	// Set default limit if not provided
	if req.Limit == 0 {
		req.Limit = 100
	}

	// Build query based on whether email filter is provided
	var query string
	var params map[string]interface{}
	
	if req.Email != "" {
		query = `
		SELECT id, email, name, created_at, updated_at
		FROM users
		WHERE LOWER(email) LIKE LOWER(@email)
		ORDER BY created_at DESC
		LIMIT @limit OFFSET @offset`
		
		params = map[string]interface{}{
			"email":  "%" + req.Email + "%",
			"limit":  req.Limit,
			"offset": req.Offset,
		}
	} else {
		query = `
		SELECT id, email, name, created_at, updated_at
		FROM users
		ORDER BY created_at DESC
		LIMIT @limit OFFSET @offset`
		
		params = map[string]interface{}{
			"limit":  req.Limit,
			"offset": req.Offset,
		}
	}

	stmt := spanner.Statement{
		SQL:    query,
		Params: params,
	}

	// Use read-only transaction for better performance
	txn := client.ReadOnlyTransaction()
	defer txn.Close()

	var users []User
	iter := txn.Query(ctx, stmt)
	defer iter.Stop()

	for {
		row, err := iter.Next()
		if err == iterator.Done {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("failed to iterate users: %w", err)
		}

		var user User
		err = row.Columns(&user.ID, &user.Email, &user.Name, &user.CreatedAt, &user.UpdatedAt)
		if err != nil {
			return nil, fmt.Errorf("failed to scan user: %w", err)
		}
		users = append(users, user)
	}

	// Get total count with a separate query
	countQuery := `SELECT COUNT(*) FROM users`
	if req.Email != "" {
		countQuery = `SELECT COUNT(*) FROM users WHERE LOWER(email) LIKE LOWER(@email)`
	}
	
	countStmt := spanner.Statement{
		SQL:    countQuery,
		Params: params,
	}
	
	var totalCount int64
	countIter := txn.Query(ctx, countStmt)
	defer countIter.Stop()
	
	row, err := countIter.Next()
	if err != nil {
		return nil, fmt.Errorf("failed to get total count: %w", err)
	}
	
	err = row.Columns(&totalCount)
	if err != nil {
		return nil, fmt.Errorf("failed to scan total count: %w", err)
	}

	return &ListUsersResponse{
		Users:      users,
		TotalCount: int(totalCount),
	}, nil
}
