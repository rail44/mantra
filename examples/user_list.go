package examples

import "time"

// ListUsersRequest represents a request to list users
// @description List users with pagination from Spanner users table
type ListUsersRequest struct {
	Limit  int    `json:"limit"`  // Maximum number of users to return (default: 100)
	Offset int    `json:"offset"` // Number of users to skip
	Email  string `json:"email"`  // Optional: filter by email (partial match)
}

// ListUsersResponse contains the list of users
type ListUsersResponse struct {
	Users      []User `json:"users"`
	TotalCount int    `json:"total_count"`
}

// User represents user information
type User struct {
	ID        string    `json:"id"`
	Email     string    `json:"email"`
	Name      string    `json:"name"`
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}