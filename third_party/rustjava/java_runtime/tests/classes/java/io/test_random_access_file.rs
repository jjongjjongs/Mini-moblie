use alloc::vec;
use alloc::vec::Vec;

use jvm::{Result, runtime::JavaLangString};

use test_utils::test_jvm_filesystem;

#[tokio::test]
async fn test_random_access_file() -> Result<()> {
    let filesystem = [("test.txt".into(), b"hello world".to_vec())];
    let jvm = test_jvm_filesystem(filesystem.into_iter().collect()).await?;

    let file = JavaLangString::from_rust_string(&jvm, "test.txt").await?;
    let mode = JavaLangString::from_rust_string(&jvm, "r").await?;

    let raf = jvm
        .new_class("java/io/RandomAccessFile", "(Ljava/lang/String;Ljava/lang/String;)V", (file, mode))
        .await?;

    let buf = jvm.instantiate_array("B", 11).await?;
    let read: i32 = jvm.invoke_virtual(&raf, "read", "([B)I", (buf.clone(),)).await?;
    assert_eq!(read, 11);

    let mut rust_buf = vec![0; 11];
    jvm.array_raw_buffer(&buf).await?.read(0, &mut rust_buf).unwrap();
    assert_eq!(rust_buf, vec![104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100]);

    Ok(())
}

/// Reading at the end of the file is -1, not 0.
///
/// A caller that loops until -1 - which is what the contract asks it to do -
/// never stops on a 0, and a WIPI title streaming a resource showed exactly
/// that: nine thousand reads of the same 5904 bytes at the same offset, all
/// answered 0, none of them moving.
#[tokio::test]
async fn test_random_access_file_read_at_end_is_minus_one() -> Result<()> {
    let filesystem = [("test.txt".into(), b"hello".to_vec())];
    let jvm = test_jvm_filesystem(filesystem.into_iter().collect()).await?;

    let file = JavaLangString::from_rust_string(&jvm, "test.txt").await?;
    let mode = JavaLangString::from_rust_string(&jvm, "r").await?;

    let raf = jvm
        .new_class("java/io/RandomAccessFile", "(Ljava/lang/String;Ljava/lang/String;)V", (file, mode))
        .await?;

    let buf = jvm.instantiate_array("B", 8).await?;

    let read: i32 = jvm.invoke_virtual(&raf, "read", "([BII)I", (buf.clone(), 0, 8)).await?;
    assert_eq!(read, 5);

    // And again, with the file already spent.
    let read: i32 = jvm.invoke_virtual(&raf, "read", "([BII)I", (buf.clone(), 0, 8)).await?;
    assert_eq!(read, -1);

    // Every time, so a loop that keeps asking keeps being told.
    let read: i32 = jvm.invoke_virtual(&raf, "read", "([BII)I", (buf, 0, 8)).await?;
    assert_eq!(read, -1);

    Ok(())
}

/// A read shorter than it asked for leaves the rest of the range alone.
///
/// The buffer it is handed is the caller's, and the bytes past what was read
/// are the caller's too - filling them with the zeroes the read buffer was
/// allocated with wipes whatever was there.
#[tokio::test]
async fn test_random_access_file_short_read_leaves_the_rest_of_the_buffer() -> Result<()> {
    let filesystem = [("test.txt".into(), b"hi".to_vec())];
    let jvm = test_jvm_filesystem(filesystem.into_iter().collect()).await?;

    let file = JavaLangString::from_rust_string(&jvm, "test.txt").await?;
    let mode = JavaLangString::from_rust_string(&jvm, "r").await?;

    let raf = jvm
        .new_class("java/io/RandomAccessFile", "(Ljava/lang/String;Ljava/lang/String;)V", (file, mode))
        .await?;

    let mut buf = jvm.instantiate_array("B", 6).await?;
    jvm.store_array(&mut buf, 0, vec![9i8, 9, 9, 9, 9, 9]).await?;

    // Two bytes into the middle of a buffer that is full of nines.
    let read: i32 = jvm.invoke_virtual(&raf, "read", "([BII)I", (buf.clone(), 2, 4)).await?;
    assert_eq!(read, 2);

    let mut rust_buf = vec![0u8; 6];
    jvm.array_raw_buffer(&buf).await?.read(0, &mut rust_buf).unwrap();
    assert_eq!(rust_buf, vec![9, 9, b'h', b'i', 9, 9]);

    Ok(())
}

/// Asking for nothing reads nothing, and says so with a zero rather than the
/// end of the file.
#[tokio::test]
async fn test_random_access_file_zero_length_read_is_zero() -> Result<()> {
    let filesystem = [("test.txt".into(), b"hello".to_vec())];
    let jvm = test_jvm_filesystem(filesystem.into_iter().collect()).await?;

    let file = JavaLangString::from_rust_string(&jvm, "test.txt").await?;
    let mode = JavaLangString::from_rust_string(&jvm, "r").await?;

    let raf = jvm
        .new_class("java/io/RandomAccessFile", "(Ljava/lang/String;Ljava/lang/String;)V", (file, mode))
        .await?;

    let buf = jvm.instantiate_array("B", 4).await?;
    let read: i32 = jvm.invoke_virtual(&raf, "read", "([BII)I", (buf, 0, 0)).await?;
    assert_eq!(read, 0);

    Ok(())
}
