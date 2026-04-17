package main

import (
	"context"
	"fmt"
	"log"
	"time"
)

// TaskRunner executes background tasks with timeout support.
// WHY: We roll our own instead of using a library because we need
// fine-grained control over cancellation propagation.
type TaskRunner struct {
	Name    string
	Timeout time.Duration
}

// Result holds the outcome of a single task execution.
type Result struct {
	TaskID  string
	Elapsed time.Duration
	Err     error
}

// Run executes fn within the configured timeout.
// NOTE: The context is derived from the parent — never pass context.Background() here.
func (tr *TaskRunner) Run(ctx context.Context, fn func(context.Context) error) Result {
	start := time.Now()
	ctx, cancel := context.WithTimeout(ctx, tr.Timeout)
	defer cancel()

	err := fn(ctx)
	return Result{TaskID: tr.Name, Elapsed: time.Since(start), Err: err}
}

func main() {
	runner := &TaskRunner{Name: "indexer", Timeout: 30 * time.Second}
	res := runner.Run(context.Background(), func(ctx context.Context) error {
		fmt.Println("indexing…")
		time.Sleep(1 * time.Second)
		return nil
	})
	log.Printf("task=%s elapsed=%v err=%v", res.TaskID, res.Elapsed, res.Err)
}
