use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::ClassAccessFlags;

use wie_jvm_support::WieJavaClassProto;

// The Generic Connection Framework's connection interfaces. Nothing here hands
// one out - `Connector` refuses every name - but a title's own classes name
// them in fields and interface calls, and those have to resolve.

// interface javax.microedition.io.Connection
pub struct Connection;

impl Connection {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/io/Connection",
            parent_class: None,
            interfaces: vec![],
            methods: vec![JavaMethodProto::new_abstract("close", "()V", Default::default())],
            fields: vec![],
            access_flags: ClassAccessFlags::INTERFACE,
        }
    }
}

// interface javax.microedition.io.InputConnection
pub struct InputConnection;

impl InputConnection {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/io/InputConnection",
            parent_class: None,
            interfaces: vec!["javax/microedition/io/Connection"],
            methods: vec![
                JavaMethodProto::new_abstract("openInputStream", "()Ljava/io/InputStream;", Default::default()),
                JavaMethodProto::new_abstract("openDataInputStream", "()Ljava/io/DataInputStream;", Default::default()),
            ],
            fields: vec![],
            access_flags: ClassAccessFlags::INTERFACE,
        }
    }
}

// interface javax.microedition.io.OutputConnection
pub struct OutputConnection;

impl OutputConnection {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/io/OutputConnection",
            parent_class: None,
            interfaces: vec!["javax/microedition/io/Connection"],
            methods: vec![
                JavaMethodProto::new_abstract("openOutputStream", "()Ljava/io/OutputStream;", Default::default()),
                JavaMethodProto::new_abstract("openDataOutputStream", "()Ljava/io/DataOutputStream;", Default::default()),
            ],
            fields: vec![],
            access_flags: ClassAccessFlags::INTERFACE,
        }
    }
}

// interface javax.microedition.io.StreamConnection
pub struct StreamConnection;

impl StreamConnection {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/io/StreamConnection",
            parent_class: None,
            interfaces: vec!["javax/microedition/io/InputConnection", "javax/microedition/io/OutputConnection"],
            methods: vec![],
            fields: vec![],
            access_flags: ClassAccessFlags::INTERFACE,
        }
    }
}
