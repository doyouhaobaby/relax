// rejects: type annotations needed
// (rustc: E0282; makepad: E0283, the `_: Sized` it cannot satisfy)
// rustc_ast_lowering/src/expr.rs: ? emits FromResidual obligations. It does
// not choose the source error type when the closure's result is unconstrained.
fn main() {
    let _closure = || { let value: Result<u32, u8> = Ok(1); Ok::<u32, _>(value?) };
}
