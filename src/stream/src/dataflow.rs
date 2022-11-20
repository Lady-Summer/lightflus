use std::{cell::RefCell, collections::BTreeMap};

use common::types::{NodeIdx, TypedValue};
use proto::common::{
    filter, flat_map, key_by, mapper, operator_info::Details, reducer, Entry, KeyedDataEvent,
    OperatorInfo,
};
use v8::HandleScope;

use crate::{err::RunnableTaskError, state, v8_runtime::RuntimeEngine};

pub struct DataflowTask<'s, 'i, S: state::StateManager>
where
    's: 'i,
{
    operator: OperatorImpl<S>,
    rt_engine: RefCell<RuntimeEngine<'s, 'i>>,
}

impl<'s, 'i, S: state::StateManager> DataflowTask<'s, 'i, S>
where
    's: 'i,
{
    pub fn new(
        op_info: OperatorInfo,
        state_manager: S,
        scope: &'i mut HandleScope<'s, ()>,
    ) -> Self {
        let (rt_engine, operator) = if op_info.clone().details.is_some() {
            let detail = op_info.clone().details.unwrap();
            match detail {
                Details::Mapper(map_value) => match map_value.value.as_ref() {
                    Some(mapper::Value::Func(map_func)) => (
                        RefCell::new(RuntimeEngine::new(
                            &map_func.function,
                            &get_function_name(&op_info),
                            scope,
                        )),
                        OperatorImpl::Map(MapOperator::new(&op_info, state_manager)),
                    ),
                    None => (
                        RefCell::new(RuntimeEngine::new("", "", scope)),
                        OperatorImpl::Empty,
                    ),
                },
                Details::Filter(filter_value) => match filter_value.value.as_ref() {
                    Some(filter::Value::Func(filter_func)) => (
                        RefCell::new(RuntimeEngine::new(
                            &filter_func.function,
                            &get_function_name(&op_info),
                            scope,
                        )),
                        OperatorImpl::Filter(FilterOperator::new(&op_info, state_manager)),
                    ),
                    None => (
                        RefCell::new(RuntimeEngine::new("", "", scope)),
                        OperatorImpl::Empty,
                    ),
                },
                Details::KeyBy(key_by_value) => match key_by_value.value.as_ref() {
                    Some(key_by::Value::Func(key_by_func)) => (
                        RefCell::new(RuntimeEngine::new(
                            &key_by_func.function,
                            &get_function_name(&op_info),
                            scope,
                        )),
                        OperatorImpl::KeyBy(KeyByOperator::new(&op_info, state_manager)),
                    ),
                    None => (
                        RefCell::new(RuntimeEngine::new("", "", scope)),
                        OperatorImpl::Empty,
                    ),
                },
                Details::Reducer(reduce_value) => match reduce_value.value.as_ref() {
                    Some(reducer::Value::Func(reduce_fn)) => (
                        RefCell::new(RuntimeEngine::new(
                            &reduce_fn.function,
                            &get_function_name(&op_info),
                            scope,
                        )),
                        OperatorImpl::Reduce(ReduceOperator::new(&op_info, state_manager)),
                    ),
                    None => (
                        RefCell::new(RuntimeEngine::new("", "", scope)),
                        OperatorImpl::Empty,
                    ),
                },
                Details::FlatMap(flat_map_value) => match flat_map_value.value.as_ref() {
                    Some(flat_map::Value::Func(flat_map_fn)) => (
                        RefCell::new(RuntimeEngine::new(
                            &flat_map_fn.function,
                            &get_function_name(&op_info),
                            scope,
                        )),
                        OperatorImpl::FlatMap(FlatMapOperator::new(&op_info, state_manager)),
                    ),
                    None => (
                        RefCell::new(RuntimeEngine::new("", "", scope)),
                        OperatorImpl::Empty,
                    ),
                },
                _ => (
                    RefCell::new(RuntimeEngine::new("", "", scope)),
                    OperatorImpl::Empty,
                ),
            }
        } else {
            (
                RefCell::new(RuntimeEngine::new("", "", scope)),
                OperatorImpl::Empty,
            )
        };
        Self {
            rt_engine,
            operator,
        }
    }

    pub(crate) fn process(
        &self,
        event: &KeyedDataEvent,
    ) -> Result<Vec<KeyedDataEvent>, RunnableTaskError> {
        self.operator.call_fn(event, &self.rt_engine)
    }
}

fn get_function_name(op_info: &OperatorInfo) -> String {
    match op_info.details.as_ref() {
        Some(info) => match info {
            Details::Mapper(_) => format!("_operator_{}_process", "map"),
            Details::Filter(_) => format!("_operator_{}_process", "filter"),
            Details::KeyBy(_) => format!("_operator_{}_process", "keyBy"),
            Details::Reducer(_) => format!("_operator_{}_process", "reduce"),
            Details::FlatMap(_) => format!("_operator_{}_process", "flatMap"),
            _ => "".to_string(),
        },
        None => "".to_string(),
    }
}

pub(crate) trait IOperator {
    fn call_fn<'p, 'i>(
        &self,
        event: &KeyedDataEvent,
        rt_engine: &RefCell<RuntimeEngine<'p, 'i>>,
    ) -> Result<Vec<KeyedDataEvent>, RunnableTaskError>
    where
        'p: 'i;
}

pub(crate) enum OperatorImpl<S: state::StateManager> {
    Map(MapOperator<S>),
    Filter(FilterOperator<S>),
    KeyBy(KeyByOperator<S>),
    FlatMap(FlatMapOperator<S>),
    Reduce(ReduceOperator<S>),
    Empty,
}

impl<S: state::StateManager> IOperator for OperatorImpl<S> {
    fn call_fn<'p, 'i>(
        &self,
        event: &KeyedDataEvent,
        rt_engine: &RefCell<RuntimeEngine<'p, 'i>>,
    ) -> Result<Vec<KeyedDataEvent>, RunnableTaskError>
    where
        'p: 'i,
    {
        match self {
            Self::Map(op) => op.call_fn(event, rt_engine),
            Self::Filter(op) => op.call_fn(event, rt_engine),
            Self::KeyBy(op) => op.call_fn(event, rt_engine),
            Self::FlatMap(op) => op.call_fn(event, rt_engine),
            Self::Reduce(op) => op.call_fn(event, rt_engine),
            Self::Empty => todo!(),
        }
    }
}

impl<S: state::StateManager> IOperator for KeyByOperator<S> {
    fn call_fn<'p, 'i>(
        &self,
        event: &KeyedDataEvent,
        rt_engine: &RefCell<RuntimeEngine<'p, 'i>>,
    ) -> Result<Vec<KeyedDataEvent>, RunnableTaskError>
    where
        'p: 'i,
    {
        let mut new_events = BTreeMap::<TypedValue, KeyedDataEvent>::new();

        event
            .data
            .iter()
            .map(|entry| TypedValue::from(entry))
            .map(|typed_val| {
                (
                    rt_engine
                        .borrow_mut()
                        .call_one_arg(&typed_val)
                        .unwrap_or(TypedValue::Invalid),
                    typed_val,
                )
            })
            .map(|(key, val)| {
                let mut value_entry = Entry::default();

                value_entry.set_data_type(val.get_type());
                value_entry.value = val.get_data();
                (key, value_entry)
            })
            .for_each(|(key, value)| {
                if !new_events.contains_key(&key) {
                    let mut key_entry = Entry::default();
                    key_entry.set_data_type(key.get_type());
                    key_entry.value = key.get_data();

                    let mut event = KeyedDataEvent::default();
                    event.from_operator_id = self.operator_id;
                    event.key = Some(key_entry);
                    new_events.insert(key.clone(), event);
                }

                match new_events.get_mut(&key) {
                    Some(event) => {
                        event.data.push(value);
                    }
                    None => {}
                }
            });

        Ok(Vec::from_iter(
            new_events.iter().map(|entry| entry.1.clone()),
        ))
    }
}

impl<S: state::StateManager> IOperator for ReduceOperator<S> {
    fn call_fn<'p, 'i>(
        &self,
        event: &KeyedDataEvent,
        rt_engine: &RefCell<RuntimeEngine<'p, 'i>>,
    ) -> Result<Vec<KeyedDataEvent>, RunnableTaskError>
    where
        'p: 'i,
    {
        let key = get_operator_state_key(self.operator_id, "reduce");
        let state = self.state_manager.get_keyed_state(key.as_slice());

        let accum = if state.is_empty() {
            event
                .data
                .iter()
                .map(|entry| TypedValue::from(entry))
                .reduce(|prev, next| {
                    rt_engine
                        .borrow_mut()
                        .call_two_args((&prev, &next))
                        .unwrap_or(TypedValue::Invalid)
                })
                .unwrap_or(TypedValue::Invalid)
        } else {
            let accum = TypedValue::from_vec(&state);

            event.data.iter().fold(accum, |accum, entry| {
                let val = TypedValue::from_slice(&entry.value);
                rt_engine
                    .borrow_mut()
                    .call_two_args((&accum, &val))
                    .unwrap_or(TypedValue::Invalid)
            })
        };

        self.state_manager
            .set_key_state(key.as_slice(), accum.get_data().as_slice());

        let mut new_event = event.clone();
        let mut entry = Entry::default();
        entry.value = accum.get_data();
        entry.set_data_type(accum.get_type());
        new_event.data = vec![entry];
        Ok(vec![new_event])
    }
}

impl<S: state::StateManager> IOperator for FilterOperator<S> {
    fn call_fn<'p, 'i>(
        &self,
        event: &KeyedDataEvent,
        rt_engine: &RefCell<RuntimeEngine<'p, 'i>>,
    ) -> Result<Vec<KeyedDataEvent>, RunnableTaskError>
    where
        'p: 'i,
    {
        let filtered = event
            .data
            .iter()
            .filter(|entry| {
                let val = TypedValue::from_slice(&entry.value);
                let result = rt_engine
                    .borrow_mut()
                    .call_one_arg(&val)
                    .unwrap_or(TypedValue::Boolean(false));
                match result {
                    TypedValue::Boolean(flag) => flag,
                    _ => false,
                }
            })
            .map(|entry| entry.clone());

        let mut new_event = event.clone();
        new_event.data = filtered.collect();
        Ok(vec![new_event])
    }
}

impl<S: state::StateManager> IOperator for FlatMapOperator<S> {
    fn call_fn<'p, 'i>(
        &self,
        event: &KeyedDataEvent,
        rt_engine: &RefCell<RuntimeEngine<'p, 'i>>,
    ) -> Result<Vec<KeyedDataEvent>, RunnableTaskError>
    where
        'p: 'i,
    {
        let flat_map_value = event.data.iter().flat_map(|entry| {
            let val = TypedValue::from_slice(&entry.value);
            let result = rt_engine
                .borrow_mut()
                .call_one_arg(&val)
                .unwrap_or(TypedValue::Array(vec![]));

            match result {
                TypedValue::Array(v) => v,
                _ => vec![],
            }
        });

        let mut new_event = event.clone();
        new_event.data = flat_map_value
            .map(|val| {
                let mut entry = Entry::default();
                entry.set_data_type(val.get_type());
                entry.value = val.get_data();

                entry
            })
            .collect();

        Ok(vec![new_event])
    }
}

macro_rules! define_operator {
    ($name: ident) => {
        pub(crate) struct $name<S>
        where
            S: state::StateManager,
        {
            state_manager: S,
            operator_id: NodeIdx,
        }
    };
}

macro_rules! new_operator {
    ($name:ident) => {
        impl<S> $name<S>
        where
            S: state::StateManager,
        {
            pub(crate) fn new(op_info: &OperatorInfo, state_manager: S) -> Self {
                let operator_id = op_info.operator_id;
                $name {
                    state_manager,
                    operator_id,
                }
            }
        }
    };
}

macro_rules! stateless_operator {
    ($name: ident) => {
        impl<S: state::StateManager> IOperator for $name<S> {
            fn call_fn<'p, 'i>(
                &self,
                event: &KeyedDataEvent,
                rt_engine: &RefCell<RuntimeEngine<'p, 'i>>,
            ) -> Result<Vec<KeyedDataEvent>, RunnableTaskError>
            where
                'p: 'i,
            {
                let mut new_event = event.clone();

                let value_entry_results = event
                    .data
                    .iter()
                    .map(|entry| TypedValue::from(entry))
                    .map(|typed_val| {
                        rt_engine
                            .borrow_mut()
                            .call_one_arg(&typed_val)
                            .unwrap_or(TypedValue::Invalid)
                    })
                    .map(|val| {
                        let mut entry = Entry::default();
                        entry.set_data_type(val.get_type());
                        entry.value = val.get_data();
                        entry
                    });
                new_event.data = Vec::from_iter(value_entry_results);
                new_event.from_operator_id = self.operator_id;
                Ok(vec![new_event])
            }
        }
    };
}

define_operator!(MapOperator);
new_operator!(MapOperator);
stateless_operator!(MapOperator);

define_operator!(FlatMapOperator);
new_operator!(FlatMapOperator);

define_operator!(FilterOperator);
new_operator!(FilterOperator);

define_operator!(KeyByOperator);
new_operator!(KeyByOperator);

define_operator!(ReduceOperator);
new_operator!(ReduceOperator);

fn get_operator_state_key(operator_id: NodeIdx, operator: &str) -> Vec<u8> {
    format!("{}-{}", operator, operator_id).as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use proto::common::{
        filter, flat_map, key_by, mapper, operator_info::Details, reducer, OperatorInfo,
    };

    fn get_opeartor_code(op_info: &OperatorInfo) -> String {
        match op_info.details.as_ref() {
            Some(info) => match info {
                Details::Mapper(func) => match &func.value {
                    Some(value) => match value {
                        mapper::Value::Func(func) => func.function.clone(),
                    },
                    None => "".to_string(),
                },
                Details::Filter(func) => match &func.value {
                    Some(value) => match value {
                        filter::Value::Func(func) => func.function.clone(),
                    },
                    None => "".to_string(),
                },
                Details::KeyBy(func) => match &func.value {
                    Some(value) => match value {
                        key_by::Value::Func(func) => func.function.clone(),
                    },
                    None => "".to_string(),
                },
                Details::Reducer(func) => match &func.value {
                    Some(value) => match value {
                        reducer::Value::Func(func) => func.function.clone(),
                    },
                    None => "".to_string(),
                },
                Details::FlatMap(func) => match &func.value {
                    Some(value) => match value {
                        flat_map::Value::Func(func) => func.function.clone(),
                    },
                    None => "".to_string(),
                },
                _ => "".to_string(),
            },
            None => "".to_string(),
        }
    }

    struct SetupGuard {}

    impl Drop for SetupGuard {
        fn drop(&mut self) {}
    }

    fn setup() -> SetupGuard {
        use crate::MOD_TEST_START;
        MOD_TEST_START.call_once(|| {
            v8::V8::set_flags_from_string(
                "--no_freeze_flags_after_init --expose_gc --harmony-import-assertions --harmony-shadow-realm --allow_natives_syntax --turbo_fast_api_calls",
              );
                  v8::V8::initialize_platform(v8::new_default_platform(0, false).make_shared());
                  v8::V8::initialize();
        });
        std::env::set_var("STATE_MANAGER", "MEM");

        SetupGuard {}
    }

    #[test]
    fn test_map_operator() {
        use super::MapOperator;
        use crate::dataflow::get_function_name;
        use crate::dataflow::IOperator;
        use crate::state::MemoryStateManager;
        use crate::v8_runtime::RuntimeEngine;
        use common::types::TypedValue;
        use proto::common::Func;
        use proto::common::{Entry, KeyedDataEvent};
        use proto::common::{Mapper, OperatorInfo};
        use std::cell::RefCell;

        let _setup_guard = setup();

        let op_info = OperatorInfo {
            operator_id: 0,
            host_addr: None,
            upstreams: Default::default(),
            details: Some(Details::Mapper(Mapper {
                value: Some(mapper::Value::Func(Func {
                    function: "function _operator_map_process(a) { return a+1 }".to_string(),
                })),
            })),
        };

        let state_manager = MemoryStateManager::new();

        let isolate = &mut v8::Isolate::new(Default::default());
        let isolated_scope = &mut v8::HandleScope::new(isolate);
        let rt_engine = RefCell::new(RuntimeEngine::new(
            "function _operator_map_process(a) { return a+1 }",
            &get_function_name(&op_info),
            isolated_scope,
        ));

        let operator = MapOperator::new(&op_info, state_manager);
        let mut event = KeyedDataEvent::default();
        event.from_operator_id = 0;

        let mut entry = Entry::default();
        let val = TypedValue::Number(1.0);
        entry.set_data_type(val.get_type());
        entry.value = val.get_data();

        event.data = vec![entry.clone(), entry.clone()];

        let result = operator.call_fn(&event, &rt_engine);
        assert!(result.is_ok());
        let new_events = result.expect("");
        assert_eq!(new_events.len(), 1);

        let mut entry = Entry::default();
        let val = TypedValue::Number(2.0);
        entry.set_data_type(val.get_type());
        entry.value = val.get_data();
        assert_eq!(
            new_events[0].data.as_slice(),
            &[entry.clone(), entry.clone()]
        );
    }

    #[test]
    fn test_filter_operator() {
        use super::FilterOperator;
        use crate::dataflow::get_function_name;
        use crate::dataflow::IOperator;
        use crate::state::MemoryStateManager;
        use crate::v8_runtime::RuntimeEngine;
        use common::types::TypedValue;
        use proto::common::Func;
        use proto::common::{Entry, KeyedDataEvent};
        use proto::common::{Filter, OperatorInfo};
        use std::cell::RefCell;

        let _setup_guard = setup();

        let op_info = OperatorInfo {
            operator_id: 0,
            host_addr: None,
            upstreams: Default::default(),
            details: Some(Details::Filter(Filter {
                value: Some(filter::Value::Func(Func {
                    function: "function _operator_filter_process(a) { return a === 1 }".to_string(),
                })),
            })),
        };

        let state_manager = MemoryStateManager::new();

        let isolate = &mut v8::Isolate::new(Default::default());
        let isolated_scope = &mut v8::HandleScope::new(isolate);
        let rt_engine = RefCell::new(RuntimeEngine::new(
            "function _operator_filter_process(a) { return a === 1 }",
            &get_function_name(&op_info),
            isolated_scope,
        ));

        let operator = FilterOperator::new(&op_info, state_manager);
        let mut event = KeyedDataEvent::default();
        event.from_operator_id = 0;

        let mut entry = Entry::default();
        let val = TypedValue::Number(1.0);
        entry.set_data_type(val.get_type());
        entry.value = val.get_data();

        let mut entry_1 = entry.clone();
        let val = TypedValue::Number(2.0);
        entry_1.set_data_type(val.get_type());
        entry_1.value = val.get_data();

        event.data = vec![entry.clone(), entry.clone(), entry_1];

        let result = operator.call_fn(&event, &rt_engine);

        {
            assert!(result.is_ok());
            let new_events = result.expect("");
            assert_eq!(new_events.len(), 1);

            assert_eq!(new_events[0].data, vec![entry.clone(), entry.clone()]);
        }
    }

    #[test]
    fn test_keyby_operator() {
        use std::collections::BTreeMap;

        use super::KeyByOperator;
        use crate::dataflow::get_function_name;
        use crate::dataflow::IOperator;
        use crate::state::MemoryStateManager;
        use crate::v8_runtime::RuntimeEngine;
        use common::collections::lang::any_match;
        use common::types::TypedValue;
        use proto::common::Func;
        use proto::common::{Entry, KeyedDataEvent};
        use proto::common::{KeyBy, OperatorInfo};
        use std::cell::RefCell;

        let _setup_guard = setup();

        let op_info = OperatorInfo {
            operator_id: 0,
            host_addr: None,
            upstreams: Default::default(),
            details: Some(Details::KeyBy(KeyBy {
                value: Some(key_by::Value::Func(Func {
                    function: "function _operator_keyBy_process(a) { return a.foo }".to_string(),
                })),
            })),
        };

        let state_manager = MemoryStateManager::new();

        let isolate = &mut v8::Isolate::new(Default::default());
        let isolated_scope = &mut v8::HandleScope::new(isolate);
        let rt_engine = RefCell::new(RuntimeEngine::new(
            &get_opeartor_code(&op_info),
            &get_function_name(&op_info),
            isolated_scope,
        ));
        let operator = KeyByOperator::new(&op_info, state_manager);

        let mut event = KeyedDataEvent::default();
        event.from_operator_id = 0;

        let mut entry = Entry::default();
        let mut val = BTreeMap::default();
        val.insert("foo".to_string(), TypedValue::String("bar".to_string()));
        val.insert("foo2".to_string(), TypedValue::String("bar".to_string()));

        let val = TypedValue::Object(val);
        entry.set_data_type(val.get_type());
        entry.value = val.get_data();

        let mut entry_1 = entry.clone();
        let mut val = BTreeMap::default();
        val.insert("foo".to_string(), TypedValue::String("bar1".to_string()));
        val.insert("foo2".to_string(), TypedValue::String("bar2".to_string()));
        let val = TypedValue::Object(val);

        entry_1.set_data_type(val.get_type());
        entry_1.value = val.get_data();

        event.data = vec![entry.clone(), entry.clone(), entry_1];

        let result = operator.call_fn(&event, &rt_engine);
        {
            assert!(result.is_ok());
            let events = result.unwrap();
            assert_eq!(events.len(), 2);
            assert!(any_match(&events, |event| {
                event.key.is_some()
                    && TypedValue::from(event.key.as_ref().unwrap())
                        == TypedValue::String("bar".to_string())
            }));
            assert!(any_match(&events, |event| {
                event.key.is_some()
                    && TypedValue::from(event.key.as_ref().unwrap())
                        == TypedValue::String("bar1".to_string())
            }));
        }
    }

    #[test]
    fn test_reduce_operator() {
        use super::ReduceOperator;
        use crate::dataflow::get_function_name;
        use crate::dataflow::IOperator;
        use crate::state::MemoryStateManager;
        use crate::v8_runtime::RuntimeEngine;
        use crate::{dataflow::get_operator_state_key, state::StateManager};
        use common::types::TypedValue;
        use proto::common::Func;
        use proto::common::{Entry, KeyedDataEvent};
        use proto::common::{OperatorInfo, Reducer};
        use std::cell::RefCell;

        let _setup_guard = setup();

        let mut op_info = OperatorInfo {
            operator_id: 0,
            host_addr: None,
            upstreams: Default::default(),
            details: Some(Details::Reducer(Reducer {
                value: Some(reducer::Value::Func(Func {
                    function:
                        "function _operator_reduce_process(accum, val) { return accum + val }"
                            .to_string(),
                })),
            })),
        };

        let state_manager = MemoryStateManager::new();

        let isolate = &mut v8::Isolate::new(Default::default());
        let isolated_scope = &mut v8::HandleScope::new(isolate);
        let rt_engine = RefCell::new(RuntimeEngine::new(
            "function _operator_reduce_process(accum, val) { return accum + val }",
            &get_function_name(&op_info),
            isolated_scope,
        ));

        let operator = ReduceOperator::new(&op_info, state_manager);
        let mut event = KeyedDataEvent::default();
        event.from_operator_id = 0;

        let mut entry = Entry::default();
        let val = TypedValue::Number(1.0);
        entry.set_data_type(val.get_type());
        entry.value = val.get_data();

        let mut entry_1 = entry.clone();
        let val = TypedValue::Number(2.0);
        entry_1.set_data_type(val.get_type());
        entry_1.value = val.get_data();

        event.data = vec![entry.clone(), entry.clone(), entry_1];
        let result = operator.call_fn(&event, &rt_engine);

        {
            assert!(result.is_ok());
            let new_events = result.expect("");
            assert_eq!(new_events.len(), 1);
            let mut entry = Entry::default();
            let val = TypedValue::Number(4.0);
            entry.set_data_type(val.get_type());
            entry.value = val.get_data();

            assert_eq!(new_events[0].data, vec![entry]);
            let state = operator
                .state_manager
                .get_keyed_state(&get_operator_state_key(operator.operator_id, "reduce"));
            assert_eq!(TypedValue::from_vec(&state), val);
        }
    }

    #[test]
    fn test_flatmap_operator() {
        use super::FlatMapOperator;
        use crate::dataflow::get_function_name;
        use crate::dataflow::IOperator;
        use crate::state::MemoryStateManager;
        use crate::v8_runtime::RuntimeEngine;
        use common::types::TypedValue;
        use proto::common::Func;
        use proto::common::{Entry, KeyedDataEvent};
        use proto::common::{FlatMap, OperatorInfo};
        use std::cell::RefCell;

        let _setup_guard = setup();

        let op_info = OperatorInfo {
            operator_id: 0,
            host_addr: None,
            upstreams: Default::default(),
            details: Some(Details::FlatMap(FlatMap {
                value: Some(flat_map::Value::Func(Func {
                    function: "function _operator_flatMap_process(v) { return [v, v, 2] }"
                        .to_string(),
                })),
            })),
        };

        let state_manager = MemoryStateManager::new();

        let isolate = &mut v8::Isolate::new(Default::default());
        let isolated_scope = &mut v8::HandleScope::new(isolate);
        let rt_engine = RefCell::new(RuntimeEngine::new(
            "function _operator_flatMap_process(v) { return [v, v, 2] }",
            &get_function_name(&op_info),
            isolated_scope,
        ));
        let operator = FlatMapOperator::new(&op_info, state_manager);

        let mut event = KeyedDataEvent::default();
        event.from_operator_id = 0;

        let mut entry = Entry::default();
        let val = TypedValue::Number(1.0);
        entry.set_data_type(val.get_type());
        entry.value = val.get_data();

        event.data = vec![entry.clone()];
        let result = operator.call_fn(&event, &rt_engine);

        {
            assert!(result.is_ok());
            let new_events = result.expect("");
            assert_eq!(new_events.len(), 1);
            let mut entry_1 = Entry::default();
            let val = TypedValue::Number(1.0);
            entry_1.set_data_type(val.get_type());
            entry_1.value = val.get_data();

            let mut entry_2 = Entry::default();
            let val = TypedValue::Number(1.0);
            entry_2.set_data_type(val.get_type());
            entry_2.value = val.get_data();

            let mut entry_3 = Entry::default();
            let val = TypedValue::Number(2.0);
            entry_3.set_data_type(val.get_type());
            entry_3.value = val.get_data();

            assert_eq!(new_events[0].data, vec![entry_1, entry_2, entry_3]);
        }
    }
}
