#[derive(Clone)]
pub enum Operation {
    CreateGame {
        definition: String,
    },
    AddEntity {
        name: String,
        entity_json: String,
    },
    RemoveEntity {
        name: String,
        entity_json: String,
    },
    UpdateScript {
        entity_name: String,
        old_script: Option<String>,
        new_script: String,
    },
    SetGameState {
        key: String,
        old_value: Option<f64>,
        new_value: f64,
    },
    ResetGame,
}

impl Operation {
    pub fn description(&self) -> String {
        match self {
            Operation::CreateGame { .. } => "Create game".to_string(),
            Operation::AddEntity { name, .. } => format!("Add entity '{name}'"),
            Operation::RemoveEntity { name, .. } => format!("Remove entity '{name}'"),
            Operation::UpdateScript { entity_name, .. } => {
                format!("Update script on '{entity_name}'")
            }
            Operation::SetGameState { key, new_value, .. } => {
                format!("Set state '{key}' = {new_value}")
            }
            Operation::ResetGame => "Reset game".to_string(),
        }
    }
}

struct HistoryNode {
    operation: Operation,
    timestamp: std::time::Instant,
    parent: Option<usize>,
    children: Vec<usize>,
}

#[derive(Default)]
pub struct OperationHistory {
    nodes: Vec<HistoryNode>,
    current: Option<usize>,
    redo_stack: Vec<usize>,
}

impl OperationHistory {
    pub fn push(&mut self, operation: Operation) {
        let parent = self.current;
        let index = self.nodes.len();

        let node = HistoryNode {
            operation,
            timestamp: std::time::Instant::now(),
            parent,
            children: Vec::new(),
        };

        self.nodes.push(node);

        if let Some(parent_index) = parent {
            self.nodes[parent_index].children.push(index);
        }

        self.current = Some(index);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> Option<&Operation> {
        let current = self.current?;
        let operation = &self.nodes[current].operation;
        let parent = self.nodes[current].parent;

        self.redo_stack.push(current);
        self.current = parent;

        Some(operation)
    }

    pub fn redo(&mut self) -> Option<&Operation> {
        let redo_index = self.redo_stack.pop()?;
        self.current = Some(redo_index);
        Some(&self.nodes[redo_index].operation)
    }

    pub fn to_json(&self) -> String {
        let start = std::time::Instant::now();
        let mut entries = Vec::new();

        for (index, node) in self.nodes.iter().enumerate() {
            let age = start
                .checked_duration_since(node.timestamp)
                .unwrap_or_default();
            let is_current = self.current == Some(index);
            let can_redo = self.redo_stack.contains(&index);

            let mut entry = serde_json::Map::new();
            entry.insert("id".to_string(), serde_json::json!(index));
            entry.insert(
                "description".to_string(),
                serde_json::json!(node.operation.description()),
            );
            entry.insert(
                "seconds_ago".to_string(),
                serde_json::json!(age.as_secs()),
            );
            entry.insert("current".to_string(), serde_json::json!(is_current));
            entry.insert("can_redo".to_string(), serde_json::json!(can_redo));
            if let Some(parent) = node.parent {
                entry.insert("parent".to_string(), serde_json::json!(parent));
            }
            if !node.children.is_empty() {
                entry.insert("children".to_string(), serde_json::json!(node.children));
            }
            entries.push(serde_json::Value::Object(entry));
        }

        let result = serde_json::json!({
            "current": self.current,
            "total_operations": self.nodes.len(),
            "can_undo": self.current.is_some(),
            "can_redo": !self.redo_stack.is_empty(),
            "operations": entries,
        });

        serde_json::to_string_pretty(&result).unwrap_or_default()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.current = None;
        self.redo_stack.clear();
    }
}
