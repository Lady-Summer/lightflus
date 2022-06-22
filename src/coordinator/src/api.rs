use dataflow_api::{dataflow_coordinator_grpc, probe};
use dataflow_api::dataflow_coordinator;
use common::event;
use crate::{cluster, coord};
use common::err::Error;
use std::sync;

const SUCCESS_MSG: &str = "success";

#[derive(Clone)]
pub(crate) struct CoordinatorApiImpl {
    coordinator: sync::Arc<coord::Coordinator>,
    cluster: sync::Arc<sync::RwLock<cluster::Cluster>>,
}

impl CoordinatorApiImpl {
    pub(crate) fn new(coordinator: coord::Coordinator, cluster: cluster::Cluster) -> CoordinatorApiImpl {
        CoordinatorApiImpl {
            coordinator: sync::Arc::new(coordinator),
            cluster: sync::Arc::new(sync::RwLock::new(cluster)),
        }
    }
}

unsafe impl Send for CoordinatorApiImpl {}

unsafe impl Sync for CoordinatorApiImpl {}

impl dataflow_coordinator_grpc::CoordinatorApi for CoordinatorApiImpl {
    fn handle_event(&mut self,
                    _ctx: grpcio::RpcContext,
                    req: dataflow_coordinator::EventRequest,
                    sink: grpcio::UnarySink<dataflow_coordinator::EventResponse>) {
        let result = serde_json::from_slice::<event::TableEvent>(req.get_data());
        let mut response = dataflow_coordinator::EventResponse::default();
        match result {
            Ok(e) => match e.action() {
                event::TableAction::FormulaSubmit {
                    table_id,
                    header_id,
                    graph
                } => {
                    match self.cluster.try_read() {
                        Ok(cluster) =>
                            match self.coordinator.submit_job(table_id, header_id, graph, cluster) {
                                Ok(_) => {
                                    response.set_code(common::http::SUCCESS);
                                    response.set_msg(SUCCESS_MSG.to_string());
                                    sink.success(response);
                                }
                                Err(err) => {
                                    log::error!("fail to handle event: {:?}", err);
                                    sink.fail(grpcio::RpcStatus::new(grpcio::RpcStatusCode::INTERNAL));
                                }
                            },
                        Err(_) => {
                            sink.fail(grpcio::RpcStatus::new(grpcio::RpcStatusCode::UNAVAILABLE));
                        }
                    }
                }
                _ => {}
            },
            Err(err) => {
                log::error!("bad body: {:?}", &err);
                sink.fail(grpcio::RpcStatus::new(grpcio::RpcStatusCode::INVALID_ARGUMENT));
            }
        }
    }

    fn probe(&mut self,
             _ctx: grpcio::RpcContext,
             req: probe::ProbeRequest,
             sink: grpcio::UnarySink<probe::ProbeResponse>) {
        match req.probeType {
            probe::ProbeRequest_ProbeType::Readiness => {
                match self.cluster.try_write() {
                    Ok(mut cluster) => {
                        sink.success(probe::ProbeResponse::default());
                        cluster.probe_state();
                    }
                    Err(_) => {
                        sink.success(probe::ProbeResponse::default());
                    }
                }
            }
            probe::ProbeRequest_ProbeType::Liveness => {
                sink.success(probe::ProbeResponse::default());
            }
        }
    }
}