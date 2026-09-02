// The first Go module in this repository.
//
// It exists because Go's own standard library IS the reference Go parser. `go/parser` +
// `go/ast` + `go/token` are correct by construction and track the language as it evolves;
// every Rust-callable alternative is a reimplementation, which is why the most serious one
// is unmaintained and stuck pre-generics.
//
// Deliberately ZERO third-party dependencies: stdlib only, so there is no go.sum, no
// module download during CI, and nothing to keep in step with a lockfile.
module github.com/hoodiecollin/forgedb/tools/goguard

go 1.24
