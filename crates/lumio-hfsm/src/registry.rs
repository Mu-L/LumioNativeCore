//! Context-owned definition registry (contract §9, ADR 0010 D6).
//!
//! Compiles specs into `Arc<CompiledDefinition>` held in a kernel handle arena.
//! `lease` hands out Arc clones as in-flight read-only leases that outlive `release`.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use lumio_kernel::context::{CancelReason, ContextResource, Deadline, QuiesceReport, QuiesceState};
use lumio_kernel::error::{ErrorCategory, KernelError, KernelResult};
use lumio_kernel::handle::{ContextKey, Handle, HandleKey, TypedHandleRegistry};

use crate::compile::{CompiledDefinition, compile};
use crate::definition::DefinitionSpec;
use crate::error::{HfsmError, HfsmResult};
use crate::ids::HfsmLimits;

/// Typed handle to a compiled definition owned by one `HfsmDefinitionRegistry`.
#[derive(Clone, Copy)]
pub struct DefinitionHandle(Handle<Arc<CompiledDefinition>>);

impl DefinitionHandle {
    /// Context + slot + generation identity (kernel handle rules apply).
    pub fn key(self) -> HandleKey {
        self.0.key()
    }
}

impl fmt::Debug for DefinitionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DefinitionHandle")
            .field(&self.key())
            .finish()
    }
}

impl PartialEq for DefinitionHandle {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for DefinitionHandle {}

/// Read-only summary of a registered definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DefinitionInfo {
    pub fingerprint: u64,
    pub state_count: u32,
    pub transition_count: u32,
    pub max_depth: u32,
    pub max_plan_actions: u32,
}

impl DefinitionInfo {
    fn of(def: &CompiledDefinition) -> Self {
        Self {
            fingerprint: def.fingerprint(),
            state_count: def.state_count(),
            transition_count: def.transition_count(),
            max_depth: def.max_depth(),
            max_plan_actions: def.max_plan_actions(),
        }
    }
}

/// `ContextResource` (`name = "hfsm"`) holding compiled definitions for one context.
pub struct HfsmDefinitionRegistry {
    context: ContextKey,
    limits: HfsmLimits,
    arena: Mutex<TypedHandleRegistry<Arc<CompiledDefinition>>>,
    closing: AtomicBool,
    destroyed: AtomicBool,
}

impl HfsmDefinitionRegistry {
    pub fn new(context: ContextKey, capacity: u32, limits: HfsmLimits) -> Self {
        Self {
            context,
            limits,
            arena: Mutex::new(TypedHandleRegistry::new(context, capacity)),
            closing: AtomicBool::new(false),
            destroyed: AtomicBool::new(false),
        }
    }

    pub fn context(&self) -> ContextKey {
        self.context
    }

    pub fn limits(&self) -> &HfsmLimits {
        &self.limits
    }

    /// Compile and store. Rejected once closing (`ContextClosing`) or destroyed.
    pub fn create(&self, spec: &DefinitionSpec) -> HfsmResult<DefinitionHandle> {
        self.ensure_live()?;
        if self.closing.load(Ordering::SeqCst) {
            return Err(HfsmError::Handle(ErrorCategory::ContextClosing));
        }
        let compiled = Arc::new(compile(spec, &self.limits)?);
        let handle = self
            .arena
            .lock()
            .expect("hfsm registry lock")
            .insert(compiled)
            .map_err(handle_err)?;
        Ok(DefinitionHandle(handle))
    }

    /// Arc clone as an in-flight read-only lease; still allowed while closing.
    pub fn lease(&self, handle: DefinitionHandle) -> HfsmResult<Arc<CompiledDefinition>> {
        self.ensure_live()?;
        let arena = self.arena.lock().expect("hfsm registry lock");
        let guard = arena.borrow(handle.0).map_err(handle_err)?;
        Ok(Arc::clone(&guard))
    }

    pub fn inspect(&self, handle: DefinitionHandle) -> HfsmResult<DefinitionInfo> {
        self.ensure_live()?;
        let arena = self.arena.lock().expect("hfsm registry lock");
        let guard = arena.borrow(handle.0).map_err(handle_err)?;
        Ok(DefinitionInfo::of(&guard))
    }

    /// Drop the registry's Arc; leases already handed out stay valid.
    pub fn release(&self, handle: DefinitionHandle) -> HfsmResult<()> {
        self.ensure_live()?;
        let removed = self
            .arena
            .lock()
            .expect("hfsm registry lock")
            .remove(handle.0)
            .map_err(handle_err)?;
        drop(removed);
        Ok(())
    }

    pub fn len(&self) -> u32 {
        self.arena.lock().expect("hfsm registry lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn ensure_live(&self) -> HfsmResult<()> {
        if self.destroyed.load(Ordering::SeqCst) {
            return Err(HfsmError::Handle(ErrorCategory::ContextDestroyed));
        }
        Ok(())
    }
}

fn handle_err(err: KernelError) -> HfsmError {
    HfsmError::Handle(err.category())
}

impl ContextResource for HfsmDefinitionRegistry {
    fn name(&self) -> &'static str {
        "hfsm"
    }

    fn cancel_requested(&self, _reason: CancelReason) {
        self.closing.store(true, Ordering::SeqCst);
    }

    fn quiesce(&self, _deadline: Deadline) -> KernelResult<QuiesceReport> {
        Ok(QuiesceReport {
            state: QuiesceState::Quiesced,
        })
    }

    fn destroy(&self) -> KernelResult<()> {
        self.destroyed.store(true, Ordering::SeqCst);
        let _ = self.arena.lock().expect("hfsm registry lock").retire_all();
        Ok(())
    }
}
