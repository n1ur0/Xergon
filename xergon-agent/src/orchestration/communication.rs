//! Agent-to-Agent (A2A) Communication Protocol
//!
//! Defines message types for multi-agent orchestration including task requests,
//! responses, heartbeats, load reports, capability queries, and result broadcasts.

use serde::{Deserialize, Serialize};

/// Represents the current load of an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLoad {
    /// Number of tasks currently being executed
    pub active_tasks: u32,
    /// Number of tasks waiting in queue
    pub queue_size: u32,
    /// CPU usage as a fraction (0.0 - 1.0)
    pub cpu_usage: f64,
    /// Memory usage as a fraction (0.0 - 1.0)
    pub memory_usage: f64,
    /// GPU usage as a fraction (0.0 - 1.0)
    pub gpu_usage: f64,
    /// Estimated remaining capacity (tasks that can still be accepted)
    pub estimated_capacity: u32,
}

impl Default for AgentLoad {
    fn default() -> Self {
        Self {
            active_tasks: 0,
            queue_size: 0,
            cpu_usage: 0.0,
            memory_usage: 0.0,
            gpu_usage: 0.0,
            estimated_capacity: 10,
        }
    }
}

/// Agent-to-Agent message types for multi-agent orchestration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum A2AMessage {
    /// Request an agent to execute a task
    TaskRequest {
        task_id: String,
        payload: serde_json::Value,
        timeout_ms: u64,
    },
    /// Response from an agent completing a task
    TaskResponse {
        task_id: String,
        result: serde_json::Value,
    },
    /// Cancel a previously requested task
    TaskCancel {
        task_id: String,
        reason: String,
    },
    /// Periodic heartbeat from an agent reporting load and capabilities
    Heartbeat {
        agent_pk: String,
        load: AgentLoad,
        capabilities: Vec<String>,
    },
    /// Detailed load report from an agent
    LoadReport {
        agent_pk: String,
        active_tasks: u32,
        queue_size: u32,
        cpu: f64,
        memory: f64,
    },
    /// Query an agent's capabilities
    CapabilityQuery {
        agent_pk: String,
    },
    /// Response to a capability query
    CapabilityResponse {
        agent_pk: String,
        capabilities: Vec<AgentCapabilityInfo>,
    },
    /// Offer a task to another agent
    TaskOffer {
        task_id: String,
        offered_by: String,
        estimated_time_ms: u64,
    },
    /// Accept a task offer
    TaskAccept {
        task_id: String,
        accepted_by: String,
    },
    /// Reject a task offer
    TaskReject {
        task_id: String,
        rejected_by: String,
        reason: String,
    },
    /// Broadcast a task result to all interested agents
    ResultBroadcast {
        task_id: String,
        result: serde_json::Value,
        quality_score: f64,
    },
    /// Report an error that occurred during task execution
    ErrorReport {
        task_id: String,
        error: String,
        from: String,
    },
}

/// Simplified capability info for A2A communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilityInfo {
    pub role: String,
    pub models: Vec<String>,
    pub max_concurrent_tasks: usize,
    pub region: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a2a_message_serialization_task_request() {
        let msg = A2AMessage::TaskRequest {
            task_id: "task-123".to_string(),
            payload: serde_json::json!({"prompt": "Hello world"}),
            timeout_ms: 5000,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::TaskRequest { task_id, payload, timeout_ms } => {
                assert_eq!(task_id, "task-123");
                assert_eq!(payload["prompt"], "Hello world");
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_a2a_message_serialization_heartbeat() {
        let msg = A2AMessage::Heartbeat {
            agent_pk: "pk-abc".to_string(),
            load: AgentLoad {
                active_tasks: 3,
                queue_size: 7,
                cpu_usage: 0.65,
                memory_usage: 0.42,
                gpu_usage: 0.80,
                estimated_capacity: 5,
            },
            capabilities: vec!["ChatCompletion".to_string(), "CodeGeneration".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::Heartbeat { agent_pk, load, capabilities } => {
                assert_eq!(agent_pk, "pk-abc");
                assert_eq!(load.active_tasks, 3);
                assert_eq!(load.estimated_capacity, 5);
                assert_eq!(capabilities.len(), 2);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_a2a_message_serialization_load_report() {
        let msg = A2AMessage::LoadReport {
            agent_pk: "pk-load".to_string(),
            active_tasks: 5,
            queue_size: 10,
            cpu: 0.9,
            memory: 0.7,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::LoadReport { agent_pk, active_tasks, queue_size, cpu, memory } => {
                assert_eq!(agent_pk, "pk-load");
                assert_eq!(active_tasks, 5);
                assert_eq!(queue_size, 10);
                assert!((cpu - 0.9).abs() < f64::EPSILON);
                assert!((memory - 0.7).abs() < f64::EPSILON);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_a2a_message_serialization_capability_query_response() {
        let query = A2AMessage::CapabilityQuery {
            agent_pk: "pk-cap".to_string(),
        };
        let json = serde_json::to_string(&query).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::CapabilityQuery { agent_pk } => {
                assert_eq!(agent_pk, "pk-cap");
            }
            _ => panic!("Wrong variant"),
        }

        let response = A2AMessage::CapabilityResponse {
            agent_pk: "pk-cap".to_string(),
            capabilities: vec![AgentCapabilityInfo {
                role: "Worker".to_string(),
                models: vec!["llama-3.1-8b".to_string()],
                max_concurrent_tasks: 5,
                region: "us-east".to_string(),
            }],
        };
        let json = serde_json::to_string(&response).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::CapabilityResponse { agent_pk, capabilities } => {
                assert_eq!(agent_pk, "pk-cap");
                assert_eq!(capabilities.len(), 1);
                assert_eq!(capabilities[0].role, "Worker");
                assert_eq!(capabilities[0].models[0], "llama-3.1-8b");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_a2a_message_serialization_task_offer_accept_reject() {
        let offer = A2AMessage::TaskOffer {
            task_id: "task-offer-1".to_string(),
            offered_by: "coordinator".to_string(),
            estimated_time_ms: 2500,
        };
        let json = serde_json::to_string(&offer).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::TaskOffer { task_id, offered_by, estimated_time_ms } => {
                assert_eq!(task_id, "task-offer-1");
                assert_eq!(offered_by, "coordinator");
                assert_eq!(estimated_time_ms, 2500);
            }
            _ => panic!("Wrong variant"),
        }

        let accept = A2AMessage::TaskAccept {
            task_id: "task-offer-1".to_string(),
            accepted_by: "worker-1".to_string(),
        };
        let json = serde_json::to_string(&accept).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::TaskAccept { task_id, accepted_by } => {
                assert_eq!(task_id, "task-offer-1");
                assert_eq!(accepted_by, "worker-1");
            }
            _ => panic!("Wrong variant"),
        }

        let reject = A2AMessage::TaskReject {
            task_id: "task-offer-1".to_string(),
            rejected_by: "worker-2".to_string(),
            reason: "At capacity".to_string(),
        };
        let json = serde_json::to_string(&reject).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::TaskReject { task_id, rejected_by, reason } => {
                assert_eq!(task_id, "task-offer-1");
                assert_eq!(rejected_by, "worker-2");
                assert_eq!(reason, "At capacity");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_a2a_message_serialization_result_broadcast() {
        let msg = A2AMessage::ResultBroadcast {
            task_id: "task-broadcast-1".to_string(),
            result: serde_json::json!({"output": "42"}),
            quality_score: 0.95,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::ResultBroadcast { task_id, result, quality_score } => {
                assert_eq!(task_id, "task-broadcast-1");
                assert_eq!(result["output"], "42");
                assert!((quality_score - 0.95).abs() < f64::EPSILON);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_a2a_message_serialization_error_report() {
        let msg = A2AMessage::ErrorReport {
            task_id: "task-err".to_string(),
            error: "GPU out of memory".to_string(),
            from: "worker-3".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: A2AMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            A2AMessage::ErrorReport { task_id, error, from } => {
                assert_eq!(task_id, "task-err");
                assert_eq!(error, "GPU out of memory");
                assert_eq!(from, "worker-3");
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_agent_load_default() {
        let load = AgentLoad::default();
        assert_eq!(load.active_tasks, 0);
        assert_eq!(load.queue_size, 0);
        assert!((load.cpu_usage - 0.0).abs() < f64::EPSILON);
        assert_eq!(load.estimated_capacity, 10);
    }
}
