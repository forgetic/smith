//! Finite local commit graph for the local-host world. Skein's fake checkout
//! owns working trees and commits; this graph supplies world-owned objects and
//! remote decisions (domain/host.md, section 8).

use std::collections::BTreeMap;

use skein_fake_checkout::git::{Created, Fault, Pushed, Remote, Tree, Want, What};

#[derive(Clone, Debug)]
struct Commit {
    parent: Option<u64>,
    merging: Option<u64>,
    tree: Tree,
    message: Vec<u8>,
}

/// A small graph with one initial branch and monotonically named objects.
#[derive(Debug)]
pub(crate) struct History {
    commits: BTreeMap<u64, Commit>,
    branch: u64,
    next: u64,
}

impl History {
    pub(crate) fn new(tree: Tree) -> Self {
        Self {
            commits: BTreeMap::from([(1, Commit { parent: None, merging: None, tree, message: Vec::new() })]),
            branch: 1,
            next: 2,
        }
    }

    pub(crate) fn commit_message(&self, commit: u64) -> &[u8] {
        &self.commits.get(&commit).expect("local commit exists").message
    }

    pub(crate) fn remote_head(&self) -> u64 {
        self.branch
    }

    pub(crate) fn move_remote(&mut self) {
        let mut tree = self.tree(self.branch);
        tree.insert(b"remote.txt".to_vec(), b"another change".to_vec());
        let previous = self.branch;
        self.branch = self.store(previous, None, tree, b"remote moved").expect("remote commit exists");
    }
}

impl Remote for History {
    fn heads(&mut self, remote: &[u8]) -> Result<Vec<u64>, Fault> {
        if remote != b"repo" {
            return Err(Fault::Missing(What::Repository));
        }
        Ok(vec![self.branch])
    }

    fn fetch(&mut self, remote: &[u8], want: Want<'_>) -> Result<u64, Fault> {
        if remote != b"repo" {
            return Err(Fault::Missing(What::Repository));
        }
        match want {
            Want::Branch(b"main") | Want::Default => Ok(self.branch),
            Want::Branch(_) => Err(Fault::Missing(What::Branch)),
            Want::Commit(commit) => {
                self.commits.contains_key(&commit).then_some(commit).ok_or(Fault::Missing(What::Commit))
            }
        }
    }

    fn create(&mut self, remote: &[u8], branch: &[u8], _commit: u64) -> Result<Created, Fault> {
        if remote != b"repo" {
            return Err(Fault::Missing(What::Repository));
        }
        if branch == b"main" { Ok(Created::Exists) } else { Err(Fault::Refused) }
    }

    fn push(&mut self, remote: &[u8], branch: &[u8], commit: u64, expected: Option<u64>) -> Result<Pushed, Fault> {
        if remote != b"repo" {
            return Err(Fault::Missing(What::Repository));
        }
        if branch != b"main" {
            return Err(Fault::Missing(What::Branch));
        }
        if expected.is_some_and(|head| self.branch != head) {
            return Ok(Pushed::Rejected);
        }
        self.branch = commit;
        Ok(Pushed::Pushed)
    }

    fn parent(&self, commit: u64) -> Option<u64> {
        self.commits.get(&commit).expect("local commit exists").parent
    }

    fn merge_parent(&self, commit: u64) -> Option<u64> {
        self.commits.get(&commit).expect("local commit exists").merging
    }

    fn tree(&self, commit: u64) -> Tree {
        self.commits.get(&commit).expect("local commit exists").tree.clone()
    }

    fn message(&self, commit: u64) -> Vec<u8> {
        self.commit_message(commit).to_vec()
    }

    fn store(&mut self, parent: u64, merging: Option<u64>, tree: Tree, message: &[u8]) -> Option<u64> {
        if merging.is_none() && self.tree(parent) == tree {
            return None;
        }
        let next = self.next;
        self.next = self.next.checked_add(1).expect("small graph");
        self.commits.insert(next, Commit { parent: Some(parent), merging, tree, message: message.to_vec() });
        Some(next)
    }
}
