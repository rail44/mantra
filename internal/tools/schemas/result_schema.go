package schemas

import (
	"encoding/json"
)

// ResultSchema defines the interface for phase result schemas
type ResultSchema interface {
	// GetSchema returns the JSON schema for validation
	GetSchema() json.RawMessage

	// Validate checks if the data conforms to the schema
	Validate(data interface{}) error

	// Transform converts the raw data into the appropriate structure
	Transform(data interface{}) (interface{}, error)
}
