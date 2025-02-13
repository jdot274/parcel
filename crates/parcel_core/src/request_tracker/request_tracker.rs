use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableDiGraph;

use super::Request;
use super::RequestEdgeType;
use super::RequestError;
use super::RequestGraph;
use super::RequestNode;
use super::RequestResult;

#[derive(Clone)]
pub struct RequestTracker<T: Clone> {
  parent_request_hash: Option<u64>,
  graph: Rc<RefCell<RequestGraph<T>>>,
  request_index: Rc<RefCell<HashMap<u64, NodeIndex>>>,
}

impl<T: Clone> RequestTracker<T> {
  pub fn new() -> Self {
    let mut graph = StableDiGraph::<RequestNode<T>, RequestEdgeType>::new();
    graph.add_node(RequestNode::Root);
    RequestTracker {
      parent_request_hash: None,
      graph: Rc::new(RefCell::new(graph)),
      request_index: Rc::new(RefCell::new(HashMap::new())),
    }
  }

  pub fn run_request(&self, request: Box<&dyn Request<T>>) -> Result<T, Vec<RequestError>> {
    let request_id = request.id();

    if self.prepare_request(request_id.clone()) {
      let mut rt = self.clone();
      rt.parent_request_hash.replace(request_id.clone());
      self.store_request(&request_id, request.run(rt));
    }

    self.get_request(&request_id)
  }

  fn prepare_request(&self, request_id: u64) -> bool {
    let mut graph = self.graph.borrow_mut();
    let mut request_index = self.request_index.borrow_mut();

    let node_index = request_index
      .entry(request_id)
      .or_insert_with(|| graph.add_node(RequestNode::Incomplete));

    let request_node = graph.node_weight_mut(*node_index).unwrap();

    // Don't run if already run
    if let RequestNode::<T>::Valid(_) = request_node {
      return false;
    }

    *request_node = RequestNode::Incomplete;
    true
  }

  fn store_request(&self, request_id: &u64, result: Result<RequestResult<T>, Vec<RequestError>>) {
    let request_index = self.request_index.borrow();
    let mut graph = self.graph.borrow_mut();

    let node_index = request_index.get(&request_id).unwrap();

    let request_node = graph.node_weight_mut(*node_index).unwrap();
    if let RequestNode::<T>::Valid(_) = request_node {
      return;
    }
    *request_node = match result {
      Ok(result) => RequestNode::Valid(result.result),
      Err(error) => RequestNode::Error(error),
    };
  }

  fn get_request(&self, request_id: &u64) -> Result<T, Vec<RequestError>> {
    let mut graph = self.graph.borrow_mut();
    let request_index = self.request_index.borrow();

    let Some(node_index) = request_index.get(&request_id) else {
      return Err(vec![RequestError::Impossible]);
    };

    if let Some(parent_request_id) = self.parent_request_hash {
      let parent_node_index = request_index.get(&parent_request_id).unwrap();
      graph.add_edge(
        parent_node_index.clone(),
        node_index.clone(),
        RequestEdgeType::SubRequest,
      );
    } else {
      graph.add_edge(
        NodeIndex::new(0),
        node_index.clone(),
        RequestEdgeType::SubRequest,
      );
    }

    let Some(request_node) = graph.node_weight(node_index.clone()) else {
      return Err(vec![RequestError::Impossible]);
    };

    match request_node {
      RequestNode::Root => Err(vec![RequestError::Impossible]),
      RequestNode::Incomplete => Err(vec![RequestError::Impossible]),
      RequestNode::Error(errors) => Err(errors.to_owned()),
      RequestNode::Valid(value) => Ok(value.clone()),
    }
  }
}
