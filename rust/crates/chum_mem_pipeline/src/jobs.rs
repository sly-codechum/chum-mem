use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCompletionJobPlan {
    pub job_type: String,
    pub dedupe_key: String,
    pub priority: i32,
    pub payload: serde_json::Value,
}

pub fn build_session_completion_job_plan(
    session_id: uuid::Uuid,
    unresolved_risk: bool,
    chroma_enabled: bool,
    knowledge_enabled: bool,
) -> Vec<SessionCompletionJobPlan> {
    let mut jobs = Vec::new();

    if knowledge_enabled {
        jobs.push(SessionCompletionJobPlan {
            job_type: "build-knowledge-graph".to_string(),
            dedupe_key: format!("knowledge:{session_id}"),
            priority: 60,
            payload: json!({
                "source": "session_end",
                "sessionId": session_id,
            }),
        });
    }

    if chroma_enabled {
        jobs.push(SessionCompletionJobPlan {
            job_type: "sync-chroma-index".to_string(),
            dedupe_key: format!("sync:{session_id}"),
            priority: 70,
            payload: json!({
                "source": "session_end",
                "sessionId": session_id,
            }),
        });
    }

    if unresolved_risk {
        jobs.push(SessionCompletionJobPlan {
            job_type: "replay-failed-session".to_string(),
            dedupe_key: format!("replay:{session_id}"),
            priority: 20,
            payload: json!({
                "source": "session_reflection_risk",
                "sessionId": session_id,
            }),
        });
    }

    jobs
}
