package examples

// Calculator provides basic arithmetic operations
type Calculator struct {
	precision int
}

// glyph: 2つの数値を加算する
func (c *Calculator) Add(a, b float64) float64 {
	panic("not implemented")
}

// glyph: 割引額を計算する
// 購入金額が10000円以上で10%割引
// 会員ランクがGoldなら追加5%割引
// 最大割引率は30%
func CalculateDiscount(amount float64, memberRank string) float64 {
	panic("not implemented")
}

// glyph: フィボナッチ数列のn番目の値を返す
// 再帰ではなく効率的な実装で
func Fibonacci(n int) int {
	panic("not implemented")
}

// glyph: 文字列が回文かどうかをチェック
// 大文字小文字は区別しない
// スペースと句読点は無視する
func IsPalindrome(s string) bool {
	panic("not implemented")
}
