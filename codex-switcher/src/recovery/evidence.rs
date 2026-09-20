use serde_json::Value;

#[derive(Default, Debug)]
pub(super) struct Evidence {
    pub(super) started: bool,
    pub(super) work: bool,
    pub(super) failed: bool,
    pub(super) aborted: bool,
    pub(super) start_time: Option<String>,
    pub(super) work_time: Option<String>,
    pub(super) start_turn_id: Option<String>,
}

impl Evidence {
    pub(super) fn event(&mut self, line: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        let payload = &value["payload"];
        let kind = payload["type"].as_str().unwrap_or("");
        let record = value["type"].as_str().unwrap_or("");
        let time = value["timestamp"].as_str().map(str::to_string);
        if record == "event_msg" && kind == "task_started" {
            self.started = true;
            self.failed = false;
            self.aborted = false;
            self.work = false;
            self.work_time = None;
            self.start_time = time;
            self.start_turn_id = payload["turn_id"].as_str().map(str::to_string);
        } else if record == "event_msg" && kind == "turn_aborted" {
            // The expected shutdown abort can appear after the pre-restart
            // checkpoint. A delayed terminal event for a different old turn
            // must not poison the IPC-confirmed replacement turn.
            let terminal_turn_id = payload["turn_id"].as_str();
            let matches_started = terminal_turn_id
                .map(|id| self.start_turn_id.as_deref() == Some(id))
                .unwrap_or(self.started);
            if self.started && matches_started {
                self.failed = true;
                self.aborted = true;
            } else if !self.started {
                self.aborted = true;
            }
        } else if record == "event_msg" && kind == "task_complete" && !payload["error"].is_null() {
            let terminal_turn_id = payload["turn_id"].as_str();
            let matches_started = terminal_turn_id
                .map(|id| self.start_turn_id.as_deref() == Some(id))
                .unwrap_or(self.started);
            if self.started && matches_started {
                self.failed = true;
            }
        } else if (record == "event_msg" && matches!(kind, "agent_message" | "agent_reasoning"))
            || (record == "response_item"
                && (matches!(
                    kind,
                    "agent_message"
                        | "reasoning"
                        | "function_call"
                        | "custom_tool_call"
                        | "web_search_call"
                ) || (kind == "message" && payload["role"].as_str() == Some("assistant"))))
        {
            self.work = true;
            self.work_time = time;
        }
    }

    pub(super) fn matches_expected_turn(&self, expected_turn_id: Option<&str>) -> bool {
        self.started
            && expected_turn_id
                .map(|expected| self.start_turn_id.as_deref() == Some(expected))
                .unwrap_or(true)
    }

    pub(super) fn verified(&self, expected_turn_id: Option<&str>) -> bool {
        // Work written after the initial checkpoint but before the old Desktop
        // finished shutting down is not recovery proof. A successful recovery
        // must contain both a fresh task_started boundary and substantive agent
        // work after that boundary.
        self.matches_expected_turn(expected_turn_id) && self.work && !self.failed && !self.aborted
    }
}
