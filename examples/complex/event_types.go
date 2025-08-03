package complex

import "time"

// OrderCreatedEvent is published when an order is created
type OrderCreatedEvent struct {
	OrderID    string
	UserID     int64
	TotalPrice float64
	Timestamp  time.Time
}

// OrderCancelledEvent is published when an order is cancelled
type OrderCancelledEvent struct {
	OrderID   string
	Reason    string
	Timestamp time.Time
}

// OrderStatusUpdatedEvent is published when order status changes
type OrderStatusUpdatedEvent struct {
	OrderID   string
	OldStatus OrderStatus
	NewStatus OrderStatus
	Timestamp time.Time
}