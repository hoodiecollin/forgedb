// Command goguard reads Go source on stdin and writes a structured verdict as JSON on
// stdout, so Rust tests can assert properties of generated Go through its AST instead of
// through substring matching.
//
// # Why a subprocess
//
// Go's standard library is the reference Go parser. Calling it from Rust means either a
// reimplementation (tree-sitter-go yields a CST and needs a C compiler; the one credible
// pure-Rust port is unmaintained and cannot parse generics) or a process boundary. The
// process boundary is the honest option: source in, verdict out, no cgo, no build.rs
// dragging an alien toolchain into cargo's dependency graph.
//
// Measured on the real generated Go SDK: ~3.75ms per call as a prebuilt binary, versus
// ~300ms under `go run`, which recompiles every time. Prebuild it.
//
// # The invariant this exists to guard
//
// CLAUDE.md's opening section: no generic runtime query surface, ever. That red line was
// guarded by five bare substrings over generated Go:
//
//	for forbidden := range []string{"forgedb_query", "switch model", "predicate", "QueryBuilder", "reflect."}
//
// It fails in BOTH directions. It fires on an innocent doc comment containing the word
// "predicate", and it misses a genuine violation spelled to dodge it:
//
//	import rt "reflect"
//	func (c *Client) Find(kind string, ms []Matcher) ([]any, error) {
//	    switch kind {
//	    case "Account": return c.scan(rt.TypeOf(Account{}), ms)
//	    }
//	}
//
// Aliasing the import and renaming one variable is the entire evasion.
//
// The lesson from building the original probe, and the reason ImportPaths is what callers
// must assert on: a first cut keyed the reflect detector on the IDENTIFIER `reflect` and
// missed the `rt` alias completely. Match the PATH, never the local name.
package main

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"io"
	"os"
	"strings"
)

// Facts is the whole wire contract with the Rust side.
type Facts struct {
	// ImportPaths holds every imported package PATH, unquoted, with the local alias
	// discarded. This is the field the identity red line asserts on: an alias cannot
	// hide a path.
	ImportPaths []string `json:"import_paths"`
	// ImportAliases maps local name -> path, for failure messages that need to explain
	// how an import was being referred to.
	ImportAliases map[string]string `json:"import_aliases"`
	// SwitchTags holds the source text of every `switch <expr>` tag expression. A
	// generic dispatcher switching on a model name shows up here regardless of what the
	// variable is called.
	SwitchTags []string `json:"switch_tags"`
	// StringSwitchTags holds only those tags whose cases include a STRING LITERAL.
	//
	// This, not SwitchTags, is what the identity red line asserts on. Generated Go
	// legitimately contains integer switches — the cgo wrappers switch on an int status
	// returned by the C ABI (`switch r { case 1: ...; case 0: ... }`), nine of them in a
	// two-model schema. Banning all dispatch would ban those, which is why "no switch at
	// all" is the wrong invariant.
	//
	// A generic model dispatcher, by contrast, must compare a model NAME, and a name is a
	// string literal in the case clause:
	//
	//	switch kind {
	//	case "Account": ...
	//	case "Project": ...
	//	}
	//
	// That is the shape with no legitimate reason to exist in per-model generated code,
	// and it is invariant under renaming both the variable and the imported package.
	StringSwitchTags []string `json:"string_switch_tags"`
	// TypeSwitches counts `switch x := y.(type)` forms, the other generic-dispatch shape.
	TypeSwitches int `json:"type_switches"`
	// DeclaredTypes, FuncNames and DeclCount describe the shape of the file, so a caller
	// can assert the parse actually saw something rather than trusting an empty verdict.
	DeclaredTypes []string `json:"declared_types"`
	FuncNames     []string `json:"func_names"`
	DeclCount     int      `json:"decl_count"`
}

func main() {
	src, err := io.ReadAll(os.Stdin)
	if err != nil {
		fail("read stdin: %v", err)
	}

	fset := token.NewFileSet()
	// ParseComments is deliberate: the file's comments are parsed and then IGNORED for
	// every fact below. That is the point — a doc comment mentioning "predicate" or
	// "QueryBuilder" is prose, and prose must not trip a guard about code.
	file, err := parser.ParseFile(fset, "input.go", src, parser.ParseComments)
	if err != nil {
		// A parse failure is a hard error, never an empty verdict. An empty verdict would
		// be indistinguishable from "clean", which is the exact failure mode the whole
		// exercise exists to delete.
		fail("parse: %v", err)
	}

	facts := Facts{
		ImportPaths:      []string{},
		ImportAliases:    map[string]string{},
		SwitchTags:       []string{},
		StringSwitchTags: []string{},
		DeclaredTypes:    []string{},
		FuncNames:        []string{},
		DeclCount:        len(file.Decls),
	}

	for _, imp := range file.Imports {
		path := strings.Trim(imp.Path.Value, `"`)
		facts.ImportPaths = append(facts.ImportPaths, path)
		if imp.Name != nil {
			facts.ImportAliases[imp.Name.Name] = path
		}
	}

	ast.Inspect(file, func(n ast.Node) bool {
		switch node := n.(type) {
		case *ast.SwitchStmt:
			if node.Tag != nil {
				tag := exprText(fset, src, node.Tag)
				facts.SwitchTags = append(facts.SwitchTags, tag)
				if hasStringCase(node) {
					facts.StringSwitchTags = append(facts.StringSwitchTags, tag)
				}
			}
		case *ast.TypeSwitchStmt:
			facts.TypeSwitches++
		case *ast.TypeSpec:
			facts.DeclaredTypes = append(facts.DeclaredTypes, node.Name.Name)
		case *ast.FuncDecl:
			facts.FuncNames = append(facts.FuncNames, node.Name.Name)
		}
		return true
	})

	out, err := json.Marshal(facts)
	if err != nil {
		fail("marshal: %v", err)
	}
	if _, err := os.Stdout.Write(out); err != nil {
		fail("write stdout: %v", err)
	}
}

// hasStringCase reports whether any case clause of this switch compares against a string
// literal. Untyped constants and identifiers do not count: only a literal, because that is
// what a model-name dispatch table is made of.
func hasStringCase(sw *ast.SwitchStmt) bool {
	for _, stmt := range sw.Body.List {
		clause, ok := stmt.(*ast.CaseClause)
		if !ok {
			continue
		}
		for _, e := range clause.List {
			if lit, ok := e.(*ast.BasicLit); ok && lit.Kind == token.STRING {
				return true
			}
		}
	}
	return false
}

// exprText renders an expression back to its original source slice, which keeps the tag
// readable in a failure message ("kind", "m.Name") rather than an AST dump.
func exprText(fset *token.FileSet, src []byte, e ast.Expr) string {
	start := fset.Position(e.Pos()).Offset
	end := fset.Position(e.End()).Offset
	if start < 0 || end > len(src) || start >= end {
		return ""
	}
	return string(src[start:end])
}

func fail(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "goguard: "+format+"\n", args...)
	os.Exit(1)
}
