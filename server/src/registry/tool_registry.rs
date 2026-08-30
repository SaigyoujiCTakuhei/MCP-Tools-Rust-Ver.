/// 工具注册中心（类比 Python ToolRegistry）
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tracing::{info, warn};

use super::tool_definition::ToolDefinition;
use crate::executor::ToolExecutor;

/// 注册表中的完整条目：定义 + 执行器 + 输入校验器 + 插件路径
pub struct ToolEntry {
    pub definition: RwLock<ToolDefinition>,
    pub executor: Box<dyn ToolExecutor>,
    /// 预编译的输入 JSON Schema 校验器（编译失败时为 None，跳过校验）
    pub validator: Option<jsonschema::Validator>,
    /// 插件二进制路径（子进程插件工具才有；热重载依据）
    pub plugin_path: Option<PathBuf>,
}

/// 工具注册中心变更事件
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ToolChangeEvent {
    Registered { name: String },
    Toggled { name: String, enabled: bool },
}

/// 工具注册中心 — 线程安全，支持并发读写
pub struct ToolRegistry {
    tools: DashMap<String, Arc<ToolEntry>>,
    /// 变更通知通道（热插拔 → WebSocket）
    notify_tx: broadcast::Sender<ToolChangeEvent>,
}

impl ToolRegistry {
    /// 创建注册中心实例
    pub fn new() -> Self {
        let (notify_tx, _) = broadcast::channel(256);
        Self {
            tools: DashMap::new(),
            notify_tx,
        }
    }

    /// 获取通知接收端（供 WebSocket 广播使用）
    pub fn notify_rx(&self) -> broadcast::Receiver<ToolChangeEvent> {
        self.notify_tx.subscribe()
    }

    /// 注册工具（定义整体传入；inputSchema 顺手预编译为输入校验器；插件工具记录二进制路径）
    pub fn register(
        &self,
        def: ToolDefinition,
        executor: Box<dyn ToolExecutor>,
        plugin_path: Option<PathBuf>,
    ) {
        let validator = jsonschema::validator_for(&def.input_schema)
            .map_err(|e| {
                warn!(tool = %def.name, error = %e, "inputSchema 编译失败，该工具将跳过输入校验");
                e
            })
            .ok();
        let name = def.name.clone();
        let entry = Arc::new(ToolEntry {
            definition: RwLock::new(def),
            executor,
            validator,
            plugin_path,
        });

        if self.tools.insert(name.clone(), entry.clone()).is_some() {
            warn!(tool = %name, "工具重复注册，已覆盖旧版本");
        }

        let _ = self.notify_tx.send(ToolChangeEvent::Registered {
            name: name.clone(),
        });

        info!(tool = %name, "工具已注册");
    }

    /// 查询工具的插件二进制路径（非插件工具返回 None）
    pub fn plugin_path(&self, name: &str) -> Option<PathBuf> {
        self.tools.get(name).and_then(|e| e.plugin_path.clone())
    }

    /// 某个二进制路径是否已被登记（扫描新插件时跳过已知项）
    pub fn has_plugin_path(&self, path: &Path) -> bool {
        self.tools.iter().any(|e| {
            e.plugin_path
                .as_ref()
                .map(|p| p == path)
                .unwrap_or(false)
        })
    }

    /// 获取工具条目
    pub fn get(&self, name: &str) -> Option<Arc<ToolEntry>> {
        self.tools.get(name).map(|r| r.value().clone())
    }

    /// 列出所有工具（仅 enabled=true 的），按名称排序保证确定性输出
    pub fn list(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> = self
            .tools
            .iter()
            .filter_map(|r| {
                let def = r.value().definition.read().ok()?;
                if def.enabled {
                    Some(def.clone())
                } else {
                    None
                }
            })
            .collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// 列出所有工具（含未启用的），按名称排序
    pub fn list_all(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> = self
            .tools
            .iter()
            .filter_map(|r| r.value().definition.read().ok().map(|d| d.clone()))
            .collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// 切换工具的启用/禁用状态
    pub fn toggle(&self, name: &str) -> Option<bool> {
        let entry = self.tools.get(name)?;
        let mut def = entry.value().definition.write().ok()?;
        def.enabled = !def.enabled;
        let new_state = def.enabled;
        drop(def);
        drop(entry); // 释放锁

        let _ = self.notify_tx.send(ToolChangeEvent::Toggled {
            name: name.to_string(),
            enabled: new_state,
        });

        info!(tool = %name, enabled = new_state, "工具状态已切换");
        Some(new_state)
    }

    /// 工具总数
    pub fn count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}