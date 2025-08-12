package schemas

import (
	"encoding/json"
)

// ResultSchema defines the interface for phase result schemas
type ResultSchema interface {
	// Schema returns the JSON schema for validation
	Schema() json.RawMessage

	// Validate checks if the data conforms to the schema
	Validate(data any) error

	// Transform converts the raw data into the appropriate structure
	Transform(data any) (any, error)
}
