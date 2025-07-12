package examples

import (
	"context"
	"fmt"
	"time"

	"cloud.google.com/go/spanner"
	"google.golang.org/api/iterator"
)

func ExecuteGetUserRequest(ctx context.Context, client *spanner.Client, req *GetUserRequest) (*GetUserResponse, error) {
	var user GetUserResponse
	err := client.Read(ctx,
		"users",
		spanner.Key{req.UserID},
		&user,
		spanner.StatementOptions{
			QueryMode: spanner.QueryModeReadOnly,
		},
	)
	if err != nil {
		return nil, fmt.Errorf("failed to read user from Spanner: %w", err)
	}
	return &user, nil
}
