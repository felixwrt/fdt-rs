//! Iterative parsers of a [`DevTree`].
use core::marker::PhantomData;
use core::mem::size_of;
use core::num::NonZeroUsize;
use core::str::from_utf8;

use crate::prelude::*;

use crate::base::parse::{next_devtree_token, ParsedTok};
use crate::base::{DevTree, DevTreeItem, DevTreeNode, DevTreeProp};
use crate::error::{DevTreeError, Result};
use crate::spec::fdt_reserve_entry;

// Re-export the basic parse iterator.
pub use super::parse::DevTreeParseIter;
pub use crate::common::prop::StringPropIter;

use fallible_iterator::FallibleIterator;

/// An iterator over [`fdt_reserve_entry`] objects within the FDT.
#[derive(Clone)]
pub struct DevTreeReserveEntryIter<'dt> {
    offset: usize,
    fdt: DevTree<'dt>,
}

#[repr(transparent)]
pub struct DevTreeReserveEntryRef<'dt>(*const fdt_reserve_entry, PhantomData<DevTree<'dt>>);

impl<'dt> DevTreeReserveEntryRef<'dt> {
    unsafe fn read_unaligned(&self) -> fdt_reserve_entry {
        self.0.read_unaligned()
    }
}

impl<'dt> DevTreeReserveEntryIter<'dt> {
    pub(crate) fn new(fdt: DevTree<'dt>) -> Self {
        Self {
            offset: fdt.off_mem_rsvmap(),
            fdt,
        }
    }

    /// Return the current offset as a fdt_reserve_entry pointer.
    unsafe fn ptr(&self) -> Result<DevTreeReserveEntryRef<'dt>> {
        Ok(DevTreeReserveEntryRef(
            self.fdt.ptr_at(self.offset)?,
            PhantomData,
        ))
    }
}

impl<'dt> Iterator for DevTreeReserveEntryIter<'dt> {
    type Item = DevTreeReserveEntryRef<'dt>;
    fn next(&mut self) -> Option<Self::Item> {
        let next_offset = size_of::<fdt_reserve_entry>() + self.offset;
        if next_offset > self.fdt.totalsize() {
            None
        } else {
            // - We previously guarunteed enough memory with next_offset check.
            // - All reads will be called through unaligned_read and are therefore
            //   safe.
            // - We will assume that given the iterator should be constructed
            //   over a valid FDT that interpretting data is valid.
            unsafe {
                let res = self.ptr().unwrap();
                let data: fdt_reserve_entry = res.read_unaligned();
                if data.address == 0.into() && data.size == 0.into() {
                    return None;
                }
                self.offset = next_offset;
                Some(res)
            }
        }
    }
}

/// An iterator over all [`DevTreeItem`] objects.
#[derive(Clone, PartialEq)]
pub struct DevTreeIter<'dt> {
    /// Offset of the last opened Device Tree Node.
    /// This is used to set properties' parent DevTreeNode.
    ///
    /// As defined by the spec, DevTreeProps must preceed Node definitions.
    /// Therefore, once a node has been closed this offset is reset to None to indicate no
    /// properties should follow.
    current_prop_parent_off: Option<NonZeroUsize>,

    /// Current offset into the flattened dt_struct section of the device tree.
    offset: usize,
    pub(crate) fdt: DevTree<'dt>,
}

#[derive(Clone, PartialEq)]
pub struct DevTreeNodeIter<'dt>(pub DevTreeIter<'dt>);
impl<'dt> FallibleIterator for DevTreeNodeIter<'dt> {
    type Item = DevTreeNode<'dt>;
    type Error = DevTreeError;
    fn next(&mut self) -> Result<Option<Self::Item>> {
        self.0.next_node()
    }
}

#[derive(Clone, PartialEq)]
pub struct DevTreePropIter<'dt>(pub DevTreeIter<'dt>);
impl<'dt> FallibleIterator for DevTreePropIter<'dt> {
    type Error = DevTreeError;
    type Item = DevTreeProp<'dt>;
    fn next(&mut self) -> Result<Option<Self::Item>> {
        self.0.next_prop()
    }
}

#[derive(Clone, PartialEq)]
pub struct DevTreeNodePropIter<'dt>(pub DevTreeIter<'dt>);
impl<'dt> FallibleIterator for DevTreeNodePropIter<'dt> {
    type Error = DevTreeError;
    type Item = DevTreeProp<'dt>;
    fn next(&mut self) -> Result<Option<Self::Item>> {
        self.0.next_node_prop()
    }
}

#[derive(Clone, PartialEq)]
pub struct DevTreeCompatibleNodeIter<'s, 'dt> {
    pub iter: DevTreeIter<'dt>,
    pub string: &'s str,
}
impl<'s, 'dt> FallibleIterator for DevTreeCompatibleNodeIter<'s, 'dt> {
    type Error = DevTreeError;
    type Item = DevTreeNode<'dt>;
    fn next(&mut self) -> Result<Option<Self::Item>> {
        self.iter.next_compatible_node(self.string)
    }
}

impl<'dt> DevTreeIter<'dt> {
    pub fn new(fdt: DevTree<'dt>) -> Self {
        Self {
            offset: fdt.off_dt_struct(),
            current_prop_parent_off: None,
            fdt,
        }
    }

    fn current_node_itr(&self) -> Option<DevTreeIter<'dt>> {
        self.current_prop_parent_off.map(|offset| DevTreeIter {
            fdt: self.fdt,
            current_prop_parent_off: Some(offset),
            offset: offset.get(),
        })
    }

    pub fn last_node(mut self) -> Option<DevTreeNode<'dt>> {
        if let Some(off) = self.current_prop_parent_off.take() {
            self.offset = off.get();
            return self.next_node().unwrap();
        }
        None
    }

    pub fn next_item(&mut self) -> Result<Option<DevTreeItem<'dt>>> {
        loop {
            let old_offset = self.offset;
            // Safe because we only pass offsets which are returned by next_devtree_token.
            let res = unsafe { next_devtree_token(self.fdt.buf(), &mut self.offset)? };

            match res {
                Some(ParsedTok::BeginNode(node)) => {
                    self.current_prop_parent_off =
                        unsafe { Some(NonZeroUsize::new_unchecked(old_offset)) };
                    return Ok(Some(DevTreeItem::Node(DevTreeNode {
                        parse_iter: self.clone(),
                        name: from_utf8(node.name).map_err(|e| e.into()),
                    })));
                }
                Some(ParsedTok::Prop(prop)) => {
                    // Prop must come after a node.
                    let prev_node = match self.current_node_itr() {
                        Some(n) => n,
                        None => return Err(DevTreeError::ParseError),
                    };

                    return Ok(Some(DevTreeItem::Prop(DevTreeProp::new(
                        prev_node,
                        prop.prop_buf,
                        prop.name_offset,
                    ))));
                }
                Some(ParsedTok::EndNode) => {
                    // The current node has ended.
                    // No properties may follow until the next node starts.
                    self.current_prop_parent_off = None;
                }
                Some(_) => continue,
                None => return Ok(None),
            }
        }
    }

    pub fn next_prop(&mut self) -> Result<Option<DevTreeProp<'dt>>> {
        loop {
            match self.next() {
                Ok(Some(DevTreeItem::Prop(p))) => return Ok(Some(p)),
                Ok(Some(_n)) => continue,
                Ok(None) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }

    pub fn next_node(&mut self) -> Result<Option<DevTreeNode<'dt>>> {
        loop {
            match self.next() {
                Ok(Some(DevTreeItem::Node(n))) => return Ok(Some(n)),
                Ok(Some(_p)) => continue,
                Ok(None) => return Ok(None),
                Err(e) => return Err(e),
            }
        }
    }

    pub fn next_node_prop(&mut self) -> Result<Option<DevTreeProp<'dt>>> {
        match self.next() {
            // Return if a new node or an EOF.
            Ok(Some(item)) => Ok(item.prop()),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn next_compatible_node(&mut self, string: &str) -> Result<Option<DevTreeNode<'dt>>> {
        // If there is another node, advance our iterator to that node.
        self.next_node().and_then(|_| {
            // Iterate through all remaining properties in the tree looking for the compatible
            // string.
            loop {
                match self.next_prop() {
                    Ok(Some(prop)) => {
                        if prop.name()? == "compatible" {
                            let mut candidates = prop.iter_str();
                            while let Some(s) = candidates.next()? {
                                if s.eq(string) {
                                    return Ok(Some(prop.node()));
                                }
                            }
                        }
                        continue;
                    }
                    Ok(None) => return Ok(None),
                    Err(e) => return Err(e),
                }
            }
        })
    }
}

impl<'dt> FallibleIterator for DevTreeIter<'dt> {
    type Error = DevTreeError;
    type Item = DevTreeItem<'dt>;

    fn next(&mut self) -> Result<Option<Self::Item>> {
        self.next_item()
    }
}
