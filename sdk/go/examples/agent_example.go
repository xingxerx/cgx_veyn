// Example: Context-aware agent that adapts behavior based on physiological state.
//
// This demonstrates a simple agent loop that:
// 1. Subscribes to real-time context updates via SSE
// 2. Adjusts response verbosity based on CognitiveLoad signal
// 3. Writes memory records on significant events
//
// Run with: go run agent_example.go

package main

import (
	"context"
	"fmt"
	"log"
	"math/rand"
	"os"
	"strings"
	"time"

	"veyn"
)

func main() {
	// Read bearer token
	tokenBytes, err := os.ReadFile(os.Getenv("HOME") + "/.local/share/veyn/token")
	if err != nil {
		log.Printf("Token not found, using mock token: %v", err)
		tokenBytes = []byte("mock-token-for-demo")
	}
	token := strings.TrimSpace(string(tokenBytes))

	client := veyn.NewClient("http://localhost:7357", token)
	ctx := context.Background()

	// Health check
	health, err := client.Health(ctx)
	if err != nil {
		log.Printf("Daemon not running, simulating context stream...")
		simulateAgentLoop(nil)
		return
	}
	fmt.Printf("✓ Connected to VEYN %s\n\n", health.Version)

	// Subscribe to context updates
	ch := make(chan veyn.ContextSnapshot)
	filter := &veyn.SubscribeFilter{
		MinConfidence: 0.4,
	}

	go func() {
		err := client.SubscribeSSE(ctx, ch, filter)
		if err != nil && err != context.Canceled {
			log.Printf("SSE error: %v", err)
		}
	}()

	fmt.Println("Listening for context updates...")
	fmt.Println("(Press Ctrl+C to stop)\n")

	simulateAgentLoop(ch)
}

func simulateAgentLoop(ch <-chan veyn.ContextSnapshot) {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case snapshot, ok := <-ch:
			if !ok {
				return
			}
			handleContextUpdate(snapshot)

		case <-ticker.C:
			// Simulate periodic agent activity
			fmt.Printf("[%s] Agent tick - checking context...\n", time.Now().Format("15:04:05"))
		}
	}
}

func handleContextUpdate(snapshot veyn.ContextSnapshot) {
	ts := time.UnixMilli(snapshot.TimestampMs)
	fmt.Printf("\n[%s] Context Update\n", ts.Format("15:04:05"))
	fmt.Printf("  Intent: %s (%s)\n", snapshot.Intent, snapshot.IntentCode)
	fmt.Printf("  Confidence: %.2f\n", snapshot.IntentConfidence)

	// Adaptive behavior based on cognitive load
	switch snapshot.IntentCode {
	case veyn.IntentCognitiveLoad:
		fmt.Println("  → Agent action: Simplifying response, reducing cognitive burden")
		adaptToCognitiveLoad(snapshot)

	case veyn.IntentFatigue:
		fmt.Println("  → Agent action: Suggesting break, deferring complex tasks")
		adaptToFatigue(snapshot)

	case veyn.IntentApproach:
		fmt.Println("  → Agent action: Surfacing relevant information for decision")
		adaptToApproach(snapshot)

	case veyn.IntentAvoidance:
		fmt.Println("  → Agent action: Gentle encouragement, breaking down task")
		adaptToAvoidance(snapshot)

	default:
		fmt.Println("  → Agent action: Normal operation")
	}

	// Print baseline deltas if available
	if len(snapshot.BaselineDelta) > 0 {
		fmt.Println("  Baseline z-scores:")
		for metric, zscore := range snapshot.BaselineDelta {
			indicator := "→"
			if zscore > 1.0 {
				indicator = "↑"
			} else if zscore < -1.0 {
				indicator = "↓"
			}
			fmt.Printf("    %s: %.2fσ %s\n", metric, zscore, indicator)
		}
	}
}

func adaptToCognitiveLoad(snapshot veyn.ContextSnapshot) {
	// When cognitive load is high, reduce information density
	responses := []string{
		"Keeping response brief due to detected cognitive load.",
		"Simplifying explanation - key point only.",
		"Deferring detailed analysis until load decreases.",
	}
	fmt.Printf("  ↳ %s\n", responses[rand.Intn(len(responses))])
}

func adaptToFatigue(snapshot veyn.ContextSnapshot) {
	// When fatigue is detected, suggest rest
	responses := []string{
		"You've been working for a while. Consider a short break?",
		"Fatigue detected. Would you like to pause and resume later?",
		"Energy levels appear low. Prioritizing rest over new tasks.",
	}
	fmt.Printf("  ↳ %s\n", responses[rand.Intn(len(responses))])
}

func adaptToApproach(snapshot veyn.ContextSnapshot) {
	// When approach motivation is detected, support the decision
	responses := []string{
		"Approach motivation detected. Surfacing supporting evidence.",
		"Positive engagement signal. Providing relevant context.",
		"Ready to proceed? Here's what you need to know.",
	}
	fmt.Printf("  ↳ %s\n", responses[rand.Intn(len(responses))])
}

func adaptToAvoidance(snapshot veyn.ContextSnapshot) {
	// When avoidance is detected, help overcome resistance
	responses := []string{
		"Avoidance signal detected. Let's break this into smaller steps.",
		"Feeling resistant? That's normal. What's the smallest next action?",
		"Resistance noted. Would a different approach help?",
	}
	fmt.Printf("  ↳ %s\n", responses[rand.Intn(len(responses))])
}
