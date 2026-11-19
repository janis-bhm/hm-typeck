# Learning about Hindley-Milner Type Inference

This project is a simple implementation of the Hindley-Milner type system for a minimal rust-like language. The ast infrastructure (code outside of `src/typeck.rs`) was written by an LLM so that I didn't have to worry about designing anything outside of the type checker.

The poly-mono part of the type checker is tested in the lambda tests: a single lambda `let lambda = |x| x`
 is polymorphic and can be used with both the `Int` and `Bool` types.
