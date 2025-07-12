package examples

import "time"

// GetUserRequest represents a request to fetch user information
// @description Retrieve user details by their unique ID from Spanner users table
type GetUserRequest struct {
	UserID string `json:"user_id"` // The unique identifier of the user
}

// GetUserResponse contains the user information
type GetUserResponse struct {
	ID        string    `json:"id"`
	Email     string    `json:"email"`
	Name      string    `json:"name"`
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}
