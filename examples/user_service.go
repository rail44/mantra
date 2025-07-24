package examples

import (
	"context"
	"time"

	"cloud.google.com/go/spanner"
)

// User represents a user in the system
type User struct {
	ID        string    `json:"id"`
	Email     string    `json:"email"`
	Name      string    `json:"name"`
	Status    string    `json:"status"`
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}

// UserService provides user-related operations
type UserService struct {
	client *spanner.Client
}

// NewUserService creates a new user service
func NewUserService(client *spanner.Client) *UserService {
	return &UserService{client: client}
}

// mantra: emailでユーザーを検索する
// Spannerのusersテーブルから検索し、idx_emailインデックスを使用
func (s *UserService) GetUserByEmail(ctx context.Context, email string) (*User, error) {
	panic("not implemented")
}

// mantra: アクティブなユーザーのリストを取得
// statusが'active'のユーザーをcreated_atの降順で取得
// 最大limit件まで返す
func (s *UserService) ListActiveUsers(ctx context.Context, limit int) ([]*User, error) {
	panic("not implemented")
}

// mantra: 新規ユーザーを作成する
// IDは自動生成し、created_atとupdated_atは現在時刻を設定
func (s *UserService) CreateUser(ctx context.Context, user *User) error {
	panic("not implemented")
}

// mantra: ユーザー情報を更新する
// updated_atを現在時刻に更新
func (s *UserService) UpdateUser(ctx context.Context, user *User) error {
	panic("not implemented")
}

// Utility functions

// mantra: 有効なメールアドレスかチェックする
// 簡単な正規表現でバリデーション
func ValidateEmail(email string) bool {
	panic("not implemented")
}

// mantra: ユーザーの表示名を生成する
// Nameがあれば使用、なければEmailのローカル部分を使用
func GetDisplayName(user *User) string {
	panic("not implemented")
}
