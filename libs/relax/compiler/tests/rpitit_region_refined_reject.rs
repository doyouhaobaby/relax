// rustc rejects: an impl's `&'b u32` for the trait's `impl Copy + 'a` without 'b: 'a (the region does not outlive)
// rejects: incompatible type for trait
#![allow(dead_code)]
trait Tr {
    fn get<'a, 'b>(x: &'a u32, y: &'b u32) -> impl Copy + 'a;
}
struct S;
impl Tr for S {
    fn get<'a, 'b>(_: &'a u32, y: &'b u32) -> &'b u32 { y }
}
