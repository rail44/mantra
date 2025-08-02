package complex

import "context"

// User represents a user
type User struct {
	ID    int64  `json:"id"`
	Name  string `json:"name"`
	Email string `json:"email"`
}

// UserRepository handles user data access
type UserRepository struct{}

// mantra: ユーザーをIDで取得する
// 存在しない場合はnilとエラーを返す
func (r *UserRepository) GetByID(ctx context.Context, id int64) (*User, error) {
	panic("not implemented")
}