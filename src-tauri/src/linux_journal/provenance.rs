use super::{
    record::{digest_valid, hash, id_valid, Content},
    state::State,
    Error, Journal, Loaded, RECORD_LIMIT,
};
use crate::linux_guard::{metadata::Security, mutation::Snapshot, Identity};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Edge {
    pub original: String,
    pub original_intent: String,
    pub original_state: String,
    pub reverse: String,
    pub reverse_intent: String,
    pub reverse_state: String,
    pub source: Identity,
    pub destination: Identity,
}
pub(super) fn validate_path(start: &Identity, end: &Identity, edges: &[Edge]) -> Result<(), Error> {
    if edges.len() > RECORD_LIMIT {
        return Err(Error::invalid("Undo provenance exceeds the bound"));
    }
    let mut current = start;
    let mut visited = vec![start];
    for edge in edges {
        if !id_valid(&edge.original)
            || !id_valid(&edge.reverse)
            || edge.original == edge.reverse
            || [
                &edge.original_intent,
                &edge.original_state,
                &edge.reverse_intent,
                &edge.reverse_state,
            ]
            .iter()
            .any(|hash| !digest_valid(hash))
            || &edge.source != current
            || visited.contains(&&edge.destination)
        {
            return Err(Error::invalid("Invalid or cyclic undo provenance"));
        }
        current = &edge.destination;
        visited.push(current);
    }
    if current != end {
        return Err(Error::invalid(
            "Undo provenance does not bind expected identity",
        ));
    }
    Ok(())
}
#[derive(Clone, PartialEq, Eq)]
struct Key {
    context: String,
    content: Content,
    security: String,
}
fn security_hash(security: &Security) -> Result<String, Error> {
    Ok(hash(&serde_json::to_vec(security).map_err(|_| {
        Error::invalid("Security serialization failed")
    })?))
}
fn key(loaded: &Loaded, content: &Content, security: &Security) -> Result<Key, Error> {
    Ok(Key {
        context: hash(
            &serde_json::to_vec(&(
                &loaded.intent.root,
                &loaded.intent.path,
                &loaded.intent.ancestors,
            ))
            .map_err(|_| Error::invalid("Context serialization failed"))?,
        ),
        content: content.clone(),
        security: security_hash(security)?,
    })
}
struct Node {
    edge: Edge,
    key: Key,
    dependencies: Vec<usize>,
    valid: bool,
}
struct Graph(Vec<Node>);
impl Graph {
    fn route(&self, wanted: &Key, start: &Identity, end: &Identity) -> Result<Vec<Edge>, Error> {
        let mut result = Vec::new();
        let mut current = start.clone();
        let mut visited = vec![current.clone()];
        while &current != end {
            if result.len() >= RECORD_LIMIT {
                return Err(Error::invalid("Undo provenance traversal bound reached"));
            }
            let candidates: Vec<_> = self
                .0
                .iter()
                .filter(|node| node.valid && node.key == *wanted && node.edge.source == current)
                .collect();
            if candidates.len() != 1 {
                return Err(Error::invalid(
                    "Missing or competing trusted undo provenance",
                ));
            }
            let edge = candidates[0].edge.clone();
            if visited.contains(&edge.destination) {
                return Err(Error::invalid("Cyclic trusted undo provenance"));
            }
            current = edge.destination.clone();
            visited.push(current.clone());
            result.push(edge);
        }
        validate_path(start, end, &result)?;
        Ok(result)
    }
    fn binding(
        &self,
        wanted: &Key,
        start: &Identity,
        end: &Identity,
        edges: &[Edge],
    ) -> Result<Vec<usize>, Error> {
        validate_path(start, end, edges)?;
        let mut dependencies = Vec::new();
        for edge in edges {
            let nodes: Vec<_> = self
                .0
                .iter()
                .enumerate()
                .filter(|(_, node)| node.key == *wanted && node.edge.source == edge.source)
                .collect();
            if nodes.len() != 1 || &nodes[0].1.edge != edge {
                return Err(Error::invalid(
                    "Undo provenance binding changed or competes",
                ));
            }
            dependencies.push(nodes[0].0);
        }
        Ok(dependencies)
    }
}
impl Journal {
    fn graph(&self) -> Result<Graph, Error> {
        let mut graph = Graph(Vec::new());
        for id in self.directory.names(RECORD_LIMIT * 2 + 1)? {
            let Ok(original) = self.load(&id) else {
                continue;
            };
            let State::Undone {
                reverse: Some(reverse),
                ..
            } = &original.revision.state
            else {
                continue;
            };
            let Ok(reversed) = self.load(reverse) else {
                continue;
            };
            if !matches!(reversed.revision.state, State::Applied { .. })
                || !self.reverse_matches(&original, &reversed)
            {
                continue;
            }
            let before = original
                .intent
                .before
                .as_ref()
                .ok_or_else(|| Error::invalid("Reverse source identity missing"))?;
            let proof = reversed
                .revision
                .state
                .proof()
                .ok_or_else(|| Error::invalid("Reverse destination identity missing"))?;
            let edge = Edge {
                original: original.intent.id.clone(),
                original_intent: original.intent_hash.clone(),
                original_state: original.revision.fingerprint()?,
                reverse: reverse.clone(),
                reverse_intent: reversed.intent_hash.clone(),
                reverse_state: reversed.revision.fingerprint()?,
                source: before.identity.clone(),
                destination: proof.file.clone(),
            };
            graph.0.push(Node {
                edge,
                key: key(&original, &before.content, &before.security)?,
                dependencies: Vec::new(),
                valid: false,
            });
            if graph.0.len() > RECORD_LIMIT {
                return Err(Error::invalid("Undo provenance graph bound reached"));
            }
        }
        let mut admitted = vec![false; graph.0.len()];
        for (index, admission) in admitted.iter_mut().enumerate() {
            let original = self
                .load(&graph.0[index].edge.original)
                .map_err(|_| Error::invalid("Undo provenance original changed"))?;
            let State::Undone {
                proof,
                identity,
                provenance,
                ..
            } = &original.revision.state
            else {
                continue;
            };
            let wanted = key(
                &original,
                &original.intent.after,
                &original.intent.after_security,
            )?;
            if let Ok(dependencies) = graph.binding(&wanted, &proof.file, identity, provenance) {
                graph.0[index].dependencies = dependencies;
                *admission = true;
            }
        }
        for _ in 0..graph.0.len() {
            let mut changed = false;
            for (index, is_admitted) in admitted.iter().enumerate() {
                if *is_admitted
                    && !graph.0[index].valid
                    && graph.0[index]
                        .dependencies
                        .iter()
                        .all(|dependency| graph.0[*dependency].valid)
                {
                    graph.0[index].valid = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(graph)
    }
    pub(super) fn expected_undo(
        &self,
        loaded: &Loaded,
        current: &Snapshot,
    ) -> Result<(Identity, Vec<Edge>), Error> {
        let Snapshot::Regular {
            identity,
            bytes,
            security,
        } = current
        else {
            return Err(Error::invalid("Undo target is missing"));
        };
        if !loaded.intent.after.matches(bytes) || &loaded.intent.after_security != security {
            return Err(Error::invalid("Later destination changes were preserved"));
        }
        let start = &loaded
            .revision
            .state
            .proof()
            .ok_or_else(|| Error::invalid("Original after identity missing"))?
            .file;
        let graph = self.graph()?;
        let wanted = key(loaded, &loaded.intent.after, &loaded.intent.after_security)?;
        let edges = if let State::Undoing {
            identity: expected,
            provenance,
            ..
        } = &loaded.revision.state
        {
            if expected != identity {
                return Err(Error::invalid("Undo expected target identity changed"));
            }
            let indices = graph.binding(&wanted, start, expected, provenance)?;
            if indices.iter().any(|index| !graph.0[*index].valid) {
                return Err(Error::invalid(
                    "Undo provenance dependencies are not verified",
                ));
            }
            provenance.clone()
        } else {
            graph.route(&wanted, start, identity)?
        };
        Ok((identity.clone(), edges))
    }
    pub(super) fn verify_undo_binding(&self, loaded: &Loaded) -> Result<(), Error> {
        let (State::Undoing {
            proof,
            identity,
            provenance,
            ..
        }
        | State::Undone {
            proof,
            identity,
            provenance,
            ..
        }) = &loaded.revision.state
        else {
            return Err(Error::invalid("Undo binding is absent"));
        };
        let graph = self.graph()?;
        let wanted = key(loaded, &loaded.intent.after, &loaded.intent.after_security)?;
        let indices = graph.binding(&wanted, &proof.file, identity, provenance)?;
        if indices.iter().any(|index| !graph.0[*index].valid) {
            return Err(Error::invalid(
                "Undo provenance dependencies are not verified",
            ));
        }
        Ok(())
    }
}
