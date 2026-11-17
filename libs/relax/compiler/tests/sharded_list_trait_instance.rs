// A safe analogue of Tokio's sharded-list trait dispatch: normalizing Target
// must not lose the impl header's implied outlives bounds.
use std::marker::PhantomData;
use std::sync::Arc;

trait Link {
    type Handle;
    type Target;
    fn target(handle: &Self::Handle) -> &Self::Target;
}
trait ShardedListItem: Link {
    fn get_shard_id(target: &Self::Target) -> usize;
}
struct Header(usize);
struct Task<S: 'static> {
    header: Arc<Header>,
    scheduler: PhantomData<S>,
}
impl<S> Link for Task<S> {
    type Handle = Task<S>;
    type Target = Header;
    fn target(handle: &Task<S>) -> &Header {
        handle.header.as_ref()
    }
}
impl<S> ShardedListItem for Task<S> {
    fn get_shard_id(target: &Self::Target) -> usize {
        target.0
    }
}
struct ShardedList<L: ShardedListItem>(PhantomData<L>);
impl<L: ShardedListItem> ShardedList<L> {
    fn lock_shard(&self, val: &L::Handle) -> usize {
        L::get_shard_id(L::target(val))
    }
}
struct Handle;
#[test]
fn generic_subtrait_method_resolves_inherited_associated_type() {
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
impl<'a, S> Link for BorrowedTask<'a, S> {
    type Handle = Self;
    type Target = Header;
    fn target(handle: &Self) -> &Header {
        &handle.header
    }
}
impl<'a, S> ShardedListItem for BorrowedTask<'a, S> {
    fn get_shard_id(target: &Self::Target) -> usize {
        target.0
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
