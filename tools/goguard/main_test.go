package main

import (
	"encoding/json"
	"os/exec"
	"strings"
	"testing"
)

// run pipes src through a freshly built goguard and decodes the verdict.
func run(t *testing.T, src string) Facts {
	t.Helper()
	cmd := exec.Command("go", "run", ".")
	cmd.Stdin = strings.NewReader(src)
	out, err := cmd.Output()
	if err != nil {
		t.Fatalf("goguard failed: %v", err)
	}
	var f Facts
	if err := json.Unmarshal(out, &f); err != nil {
		t.Fatalf("decode verdict: %v (raw: %s)", err, out)
	}
	return f
}

func has(hay []string, needle string) bool {
	for _, h := range hay {
		if h == needle {
			return true
		}
	}
	return false
}

// The FALSE-ALARM direction. Every one of the five old substrings appears here, in prose
// and in a string literal. None of it is a generic query surface.
func TestInnocentProseIsClean(t *testing.T) {
	f := run(t, `package sdk

// Find returns rows. It builds no predicate and is not a QueryBuilder;
// it does not use reflect. at all.
func Find() string {
	return "predicate QueryBuilder reflect. forgedb_query"
}
`)
	if len(f.ImportPaths) != 0 {
		t.Errorf("no imports expected, got %v", f.ImportPaths)
	}
	if len(f.SwitchTags) != 0 {
		t.Errorf("no switches expected, got %v", f.SwitchTags)
	}
	if !has(f.FuncNames, "Find") {
		t.Errorf("expected to see func Find, got %v", f.FuncNames)
	}
}

// The FALSE-GREEN direction — the dangerous one. A genuine generic runtime query surface
// spelled to dodge all five substrings: `reflect` is aliased to `rt`, and the switch tag is
// `kind` rather than `model`.
func TestProperlyEvasiveViolationIsCaught(t *testing.T) {
	f := run(t, `package sdk

import rt "reflect"

type Matcher struct{}
type Account struct{}

func Find(kind string, ms []Matcher) []any {
	switch kind {
	case "Account":
		return scan(rt.TypeOf(Account{}), ms)
	}
	return nil
}

func scan(t any, ms []Matcher) []any { return nil }
`)
	if !has(f.ImportPaths, "reflect") {
		t.Fatalf("the import PATH must be reported regardless of the alias; got %v", f.ImportPaths)
	}
	if f.ImportAliases["rt"] != "reflect" {
		t.Errorf("alias rt -> reflect must be recorded, got %v", f.ImportAliases)
	}
	if !has(f.SwitchTags, "kind") {
		t.Fatalf("the switch tag must be reported whatever it is named; got %v", f.SwitchTags)
	}
}

// The historical near-miss: this variant tripped the old substring guard, but only by luck,
// because `switch model` and `reflect.` happen to be the literal spellings it looked for.
func TestLuckyHitIsAlsoCaught(t *testing.T) {
	f := run(t, `package sdk

import "reflect"

func Find(model string) any {
	switch model {
	case "A":
		return reflect.TypeOf(0)
	}
	return nil
}
`)
	if !has(f.ImportPaths, "reflect") || !has(f.SwitchTags, "model") {
		t.Errorf("expected reflect import and `model` tag, got %v / %v", f.ImportPaths, f.SwitchTags)
	}
}

// A type switch is the other generic-dispatch shape and has no string tag at all, so
// SwitchTags alone would miss it.
func TestTypeSwitchIsCounted(t *testing.T) {
	f := run(t, `package sdk

func Find(v any) string {
	switch v.(type) {
	case int:
		return "int"
	}
	return ""
}
`)
	if f.TypeSwitches != 1 {
		t.Errorf("expected 1 type switch, got %d", f.TypeSwitches)
	}
}

// A clean generated-SDK shape: real imports, real types, no dispatch.
func TestCleanSdkShape(t *testing.T) {
	f := run(t, `package sdk

import (
	"encoding/json"
	"net/http"
)

type Client struct{ base string }
type Account struct{ ID string }

func (c *Client) GetAccount(id string) (*Account, error) {
	_ = json.Marshal
	_ = http.MethodGet
	return nil, nil
}
`)
	if has(f.ImportPaths, "reflect") {
		t.Error("no reflect expected")
	}
	if len(f.SwitchTags) != 0 || f.TypeSwitches != 0 {
		t.Errorf("no dispatch expected, got %v / %d", f.SwitchTags, f.TypeSwitches)
	}
	if !has(f.DeclaredTypes, "Account") || !has(f.FuncNames, "GetAccount") {
		t.Errorf("expected the declared shape, got %v / %v", f.DeclaredTypes, f.FuncNames)
	}
}
