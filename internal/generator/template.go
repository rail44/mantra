package generator

// DefaultTemplate provides a fallback template for generation
const DefaultTemplate = `package {{.Package}}

import (
	"context"
	"fmt"

	"cloud.google.com/go/spanner"
	"google.golang.org/api/iterator"
)

// Execute{{.RequestType}} handles the {{.RequestType}} and returns {{.ResponseType}}
func Execute{{.RequestType}}(ctx context.Context, client *spanner.Client, req *{{.RequestType}}) (*{{.ResponseType}}, error) {
	// TODO: Implement based on the request structure
	return nil, fmt.Errorf("not implemented")
}
`