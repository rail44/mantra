package phase

// Phase names
const (
	PhaseContextGathering = "Context Gathering"
	PhaseImplementation   = "Implementation"
)

// Phase states for Context Gathering
const (
	StateContextInitializing = "Initializing"
	StateContextAnalyzing    = "Analyzing codebase"
	StateContextSearching    = "Searching for types and functions"
	StateContextInspecting   = "Inspecting symbols"
	StateContextReading      = "Reading function implementations"
)

// Phase states for Implementation
const (
	StateImplPreparing  = "Preparing"
	StateImplGenerating = "Generating code"
	StateImplValidating = "Validating code"
	StateImplFinalizing = "Finalizing"
)
