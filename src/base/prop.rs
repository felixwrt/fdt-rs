use core::ptr;

use crate::base::iters::DevTreeIter;
use crate::base::{DevTree, DevTreeNode};
use crate::prelude::*;

use unsafe_unwrap::UnsafeUnwrap;

/// A handle to a [`DevTreeNode`]'s Device Tree Property
#[derive(Clone)]
pub struct DevTreeProp<'dt> {
    parent_iter: DevTreeIter<'dt>,
    propbuf: &'dt [u8],
    nameoff: usize,
}

impl<'dt> PartialEq for DevTreeProp<'dt> {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.propbuf, other.propbuf)
            && self.parent_iter == other.parent_iter
            && self.nameoff == other.nameoff
    }
}

impl<'dt> PropReader<'dt> for DevTreeProp<'dt> {
    type NodeType = DevTreeNode<'dt>;

    #[inline]
    fn propbuf(&self) -> &'dt [u8] {
        self.propbuf
    }

    #[inline]
    fn nameoff(&self) -> usize {
        self.nameoff
    }

    #[inline]
    fn fdt(&self) -> &DevTree<'dt> {
        &self.parent_iter.fdt
    }

    /// Returns the node which this property is attached to
    #[must_use]
    fn node(&self) -> DevTreeNode<'dt> {
        unsafe {
            // Unsafe unwrap okay.
            // We're look back in the tree - our parent node is behind us.
            self.parent_iter.clone().last_node().unsafe_unwrap()
        }
    }
}

impl<'dt> DevTreeProp<'dt> {
    pub(super) fn new(
        parent_iter: DevTreeIter<'dt>,
        propbuf: &'dt [u8],
        nameoff: usize,
    ) -> Self {
        Self {
            parent_iter,
            propbuf,
            nameoff,
        }
    }
}
