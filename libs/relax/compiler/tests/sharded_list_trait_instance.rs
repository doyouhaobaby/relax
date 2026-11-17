use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::Arc;

unsafe trait Link {
    type Handle;
    type Target;
    fn as_raw(handle: &Self::Handle) -> NonNull<Self::Target>;
}
unsafe trait ShardedListItem: Link {
    unsafe fn get_shard_id(target: NonNull<Self::Target>) -> usize;
}
struct Header(usize);
struct Task<S: 'static> {
    header: Arc<Header>,
    scheduler: PhantomData<S>,
}
unsafe impl<S> Link for Task<S> {
    type Handle = Task<S>;
    type Target = Header;
    fn as_raw(handle: &Task<S>) -> NonNull<Header> {
        NonNull::from(handle.header.as_ref())
    }
}
unsafe impl<S> ShardedListItem for Task<S> {
    unsafe fn get_shard_id(target: NonNull<Self::Target>) -> usize {
        unsafe { target.as_ref().0 }
    }
}
struct ShardedList<L: ShardedListItem>(PhantomData<L>);
impl<L: ShardedListItem> ShardedList<L> {
    fn lock_shard(&self, val: &L::Handle) -> usize {
        unsafe { L::get_shard_id(L::as_raw(val)) }
    }
}
struct Handle;
#[test]
fn generic_unsafe_subtrait_method_resolves_inherited_associated_type() {
    let task = Task::<Arc<Handle>> {
        header: Arc::new(Header(7)),
        scheduler: PhantomData,
    };
    let list = ShardedList::<Task<Arc<Handle>>>(PhantomData);
    assert_eq!(list.lock_shard(&task), 7);
}

// The bound can be an impl lifetime rather than 'static.
struct BorrowedTask<'a, S: 'a> {
    header: Header,
    scheduler: PhantomData<&'a S>,
}
unsafe impl<'a, S> Link for BorrowedTask<'a, S> {
    type Handle = Self;
    type Target = Header;
    fn as_raw(handle: &Self) -> NonNull<Header> {
        NonNull::from(&handle.header)
    }
}
unsafe impl<'a, S> ShardedListItem for BorrowedTask<'a, S> {
    unsafe fn get_shard_id(target: NonNull<Self::Target>) -> usize {
        unsafe { target.as_ref().0 }
    }
}
fn borrowed_shard<'a>(value: &'a usize) -> usize {
    let task = BorrowedTask::<'a, &'a usize> {
        header: Header(*value),
        scheduler: PhantomData,
    };
    ShardedList::<BorrowedTask<'a, &'a usize>>(PhantomData).lock_shard(&task)
}
#[test]
fn generic_subtrait_method_preserves_non_static_impl_bounds() {
    assert_eq!(borrowed_shard(&11), 11);
}
