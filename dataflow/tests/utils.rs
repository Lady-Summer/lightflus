use std::collections;
use dataflow::{runtime, types};

pub fn default_graph() -> runtime::Graph {
    runtime::Graph::new(
        types::job_id("tableId", "headerId"),
        default_adj_vec(),
        default_nodeset(),
    )
}


pub fn default_adj_vec() -> Vec<types::AdjacentVec> {
    vec![
        types::AdjacentVec {
            neighbors: vec![1, 2, 3],
            center: 0,
        },
        types::AdjacentVec {
            neighbors: vec![4, 5],
            center: 1,
        },
        types::AdjacentVec {
            neighbors: vec![5, 6],
            center: 2,
        },
        types::AdjacentVec {
            neighbors: vec![7],
            center: 3,
        },
        types::AdjacentVec {
            neighbors: vec![8],
            center: 6,
        },
    ]
}

pub fn default_formula_graph() -> types::formula::FormulaGraph {
    let mut values = vec![];

    for (id, operator) in default_nodeset().iter() {
        values.push((id.clone(), operator.value.clone()))
    }

    types::formula::FormulaGraph {
        meta: default_adj_vec(),
        data: collections::BTreeMap::from_iter(values),
    }
}

pub fn default_nodeset() -> types::NodeSet {
    types::NodeSet::from(
        [
            ("0".to_string(), types::Operator {
                addr: "localhost".to_string(),
                value: types::formula::FormulaOp::Reference {
                    table_id: "tableId_1".to_string(),
                    header_id: "headerId_1".to_string(),
                    value_type: types::ValueType::String,
                },
                id: 0,
            }),
            ("1".to_string(), types::Operator {
                addr: "".to_string(),
                value: types::formula::FormulaOp::Add,
                id: 1,
            }),
            ("2".to_string(), types::Operator {
                addr: "".to_string(),
                value: types::formula::FormulaOp::Add,
                id: 2,
            }),
            ("3".to_string(), types::Operator {
                addr: "".to_string(),
                value: types::formula::FormulaOp::Add,
                id: 3,
            }),
            ("4".to_string(), types::Operator {
                addr: "".to_string(),
                value: types::formula::FormulaOp::Add,
                id: 4,
            }),
            ("5".to_string(), types::Operator {
                addr: "".to_string(),
                value: types::formula::FormulaOp::Add,
                id: 5,
            }),
            ("6".to_string(), types::Operator {
                addr: "".to_string(),
                value: types::formula::FormulaOp::Add,
                id: 6,
            }),
            ("7".to_string(), types::Operator {
                addr: "".to_string(),
                value: types::formula::FormulaOp::Add,
                id: 6,
            }),
            ("8".to_string(), types::Operator {
                addr: "".to_string(),
                value: types::formula::FormulaOp::Add,
                id: 8,
            }),
        ]
    )
}


pub fn default_empty_graph() -> types::GraphModel {
    types::GraphModel {
        job_id: types::job_id("tableId", "headerId"),
        meta: vec![],
        nodes: Default::default(),
    }
}