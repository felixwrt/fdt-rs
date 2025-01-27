use crate::prelude::*;

use crate::base::{DevTreeNode, DevTreeProp};

/// An enum which contains either a [`DevTreeNode`] or a [`DevTreeProp`]
#[derive(Clone, PartialEq)]
pub enum DevTreeItem<'dt> {
    Node(DevTreeNode<'dt>),
    Prop(DevTreeProp<'dt>),
}

impl<'dt> UnwrappableDevTreeItem<'dt> for DevTreeItem<'dt> {
    type TreeNode = DevTreeNode<'dt>;
    type TreeProp = DevTreeProp<'dt>;

    #[inline]
    fn node(self) -> Option<Self::TreeNode> {
        match self {
            DevTreeItem::Node(node) => Some(node),
            _ => None,
        }
    }

    #[inline]
    fn prop(self) -> Option<Self::TreeProp> {
        match self {
            DevTreeItem::Prop(prop) => Some(prop),
            _ => None,
        }
    }
}
