use std::{collections::HashMap, sync};

use crate::cluster;
use common::{err::ApiError, net::status::{self, to_rpc_code}};
use proto::{common::stream::Dataflow, worker::{cli, worker::CreateDataflowRequest}};

pub const COORD_JOB_GRAPH_COLLECTION: &str = "coord.job.graph";

pub enum JobStorage {
    PgSQL,
}

impl JobStorage {}

pub struct Coordinator {
    job_storage: JobStorage,
    cluster: cluster::Cluster,
}

impl Coordinator {
    pub fn new(job_storage: JobStorage, cluster_config: &Vec<cluster::NodeConfig>) -> Self {
        Coordinator {
            job_storage,
            cluster: cluster::Cluster::new(cluster_config),
        }
    }

    pub fn create_dataflow(&mut self, dataflow: Dataflow) -> Result<(), ApiError> {
        let map = self.cluster.partition_dataflow(dataflow);
        for elem in map {
            let client = cli::new_dataflow_worker_client(cli::DataflowWorkerConfig {
                host: None,
                port: None,
                uri: Some(elem.0.clone()),
            });
            let ref mut req = CreateDataflowRequest::new();
            req.set_job_id(elem.1.get_job_id().clone());
            req.set_dataflow(elem.1.clone());
            match client.create_dataflow(req)
            .map_err(|err| ApiError::from(err))
            .and_then(|resp| {
                if resp.get_resp().get_status() == status::SUCCESS {
                    Ok(())
                } else {
                    Err(ApiError {
                        code: to_rpc_code(resp.get_resp().get_status()),
                        msg: resp.get_resp().get_err_msg().to_string(),
                    })
                }
            }) {
                Ok(_) => {},
                Err(err) => {
                    return Err(err)
                },
            }
        }

        Ok(())
    }
}

#[derive(serde::Deserialize, Clone, Debug)]
pub struct CoordinatorConfig {
    pub port: usize,
    pub cluster: Vec<cluster::NodeConfig>,
}

pub struct CoordinatorException {}
