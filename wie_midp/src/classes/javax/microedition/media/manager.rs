use alloc::vec;

use java_class_proto::JavaMethodProto;
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::{io::InputStream, lang::String};
use jvm::{ClassInstanceRef, Jvm, Result, runtime::JavaLangString};

use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::{
    javax::microedition::media::Player,
    net::wie::{FORMAT_MIDI, FORMAT_WAVE},
};

// class javax.microedition.media.Manager
pub struct Manager;

impl Manager {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/media/Manager",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![JavaMethodProto::new(
                "createPlayer",
                "(Ljava/io/InputStream;Ljava/lang/String;)Ljavax/microedition/media/Player;",
                Self::create_player,
                MethodAccessFlags::STATIC,
            )],
            fields: vec![],
            access_flags: Default::default(),
        }
    }

    async fn create_player(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        stream: ClassInstanceRef<InputStream>,
        r#type: ClassInstanceRef<String>,
    ) -> Result<ClassInstanceRef<Player>> {
        tracing::debug!("javax.microedition.media.Manager::createPlayer({stream:?}, {type:?})");

        let type_string = JavaLangString::to_rust_string(jvm, &r#type).await?;
        // MIDI and WAVE are what a MIDP 2.0 handset in Europe plays - 리얼풋볼
        // 매니저2010's Nokia N95 build carries nothing else - and play through
        // the same path SMAF does.
        let format = match type_string.to_ascii_lowercase().as_str() {
            "audio/midi" | "audio/mid" | "audio/x-midi" | "audio/sp-midi" => Some(FORMAT_MIDI),
            "audio/x-wav" | "audio/wav" | "audio/wave" => Some(FORMAT_WAVE),
            _ => None,
        };

        if type_string == "application/vnd.smaf" {
            Ok(jvm.new_class("net/wie/SmafPlayer", "(Ljava/io/InputStream;)V", (stream,)).await?.into())
        } else if let Some(format) = format {
            Ok(jvm
                .new_class("net/wie/SmafPlayer", "(Ljava/io/InputStream;I)V", (stream, format))
                .await?
                .into())
        } else {
            Err(jvm
                .exception(
                    "javax/microedition/media/MediaException",
                    &alloc::format!("Unsupported media type {type_string:?}"),
                )
                .await)
        }
    }
}
