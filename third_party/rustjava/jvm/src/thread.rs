use alloc::{
    boxed::Box,
    collections::VecDeque,
    string::{String, ToString},
    vec::Vec,
};

use crate::{ClassInstance, class_loader::Class};

pub enum StackFrame {
    Java(JavaStackFrame),
    Native(NativeStackFrame),
}

impl StackFrame {
    pub fn local_variables(&self) -> &[Box<dyn ClassInstance>] {
        match self {
            StackFrame::Java(java_frame) => &java_frame.local_variables,
            StackFrame::Native(native_frame) => &native_frame.local_variables,
        }
    }

    pub fn local_variables_mut(&mut self) -> &mut Vec<Box<dyn ClassInstance>> {
        match self {
            StackFrame::Java(java_frame) => &mut java_frame.local_variables,
            StackFrame::Native(native_frame) => &mut native_frame.local_variables,
        }
    }
}

/// How many of the objects most recently returned to a thread stay rooted.
/// See [`JvmThread::remember_return`].
const RECENT_RETURNS: usize = 64;

pub struct JvmThread {
    stack: Vec<StackFrame>,
    java_thread: Option<Box<dyn ClassInstance>>,
    /// The objects most recently handed back to this thread by a method it
    /// called, newest last.
    recent_returns: VecDeque<Box<dyn ClassInstance>>,
}

impl JvmThread {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            java_thread: None,
            recent_returns: VecDeque::new(),
        }
    }

    /// Keeps an object a called method returned rooted for a while.
    ///
    /// A method's own frame is what roots the objects it makes, and that frame
    /// is gone the moment it returns - so the object it returns is held by
    /// nothing but its caller's Rust variable, which no collector can see. A
    /// caller that keeps it across its next allocation (`String.valueOf`, then
    /// `new StringBuffer`, then `append`) would have it collected under it.
    /// Rooting the last few returns covers that window without growing for as
    /// long as a long-running caller - an event loop - keeps calling.
    pub fn remember_return(&mut self, object: Box<dyn ClassInstance>) {
        if self.recent_returns.len() == RECENT_RETURNS {
            self.recent_returns.pop_front();
        }
        self.recent_returns.push_back(object);
    }

    pub fn recent_returns(&self) -> impl Iterator<Item = &Box<dyn ClassInstance>> {
        self.recent_returns.iter()
    }

    #[allow(clippy::borrowed_box)] // same as jvm.rs; callers pass it to &Box-taking apis
    pub fn java_thread(&self) -> Option<&Box<dyn ClassInstance>> {
        self.java_thread.as_ref()
    }

    pub fn set_java_thread(&mut self, java_thread: Box<dyn ClassInstance>) {
        self.java_thread = Some(java_thread);
    }

    pub fn push_java_frame(&mut self, class: &Class, class_instance: Option<Box<dyn ClassInstance>>, method: &str) {
        self.stack.push(StackFrame::Java(JavaStackFrame {
            class: class.clone(),
            class_instance,
            method: method.to_string(),
            local_variables: Vec::new(),
        }));
    }

    pub fn push_native_frame(&mut self) {
        self.stack.push(StackFrame::Native(NativeStackFrame { local_variables: Vec::new() }));
    }

    pub fn pop_frame(&mut self) -> Option<StackFrame> {
        self.stack.pop()
    }

    pub fn top_frame_mut(&mut self) -> &mut StackFrame {
        self.stack.last_mut().unwrap()
    }

    pub fn top_java_frame(&self) -> Option<&JavaStackFrame> {
        self.stack.iter().rev().find_map(|frame| match frame {
            StackFrame::Java(java_frame) => Some(java_frame),
            _ => None,
        })
    }

    pub fn iter_java_frame(&self) -> impl DoubleEndedIterator<Item = &JavaStackFrame> {
        self.stack.iter().filter_map(|frame| match frame {
            StackFrame::Java(java_frame) => Some(java_frame),
            _ => None,
        })
    }

    pub fn iter_frame(&self) -> impl DoubleEndedIterator<Item = &StackFrame> {
        self.stack.iter()
    }
}

pub struct JavaStackFrame {
    pub class: Class,
    pub class_instance: Option<Box<dyn ClassInstance>>,
    pub method: String,
    pub local_variables: Vec<Box<dyn ClassInstance>>,
}

pub struct NativeStackFrame {
    pub local_variables: Vec<Box<dyn ClassInstance>>,
}
