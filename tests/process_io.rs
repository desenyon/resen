use resen::process::bounded_line;
use tokio::io::BufReader;

#[tokio::test]
async fn bounded_lines_preserve_utf8_delimiters_and_final_line() {
    let mut reader = BufReader::with_capacity(1, "研究\r\nlast".as_bytes());
    assert_eq!(
        bounded_line(&mut reader, 8).await.unwrap().as_deref(),
        Some("研究\r\n")
    );
    assert_eq!(
        bounded_line(&mut reader, 4).await.unwrap().as_deref(),
        Some("last")
    );
    assert!(bounded_line(&mut reader, 4).await.unwrap().is_none());
}

#[tokio::test]
async fn bounded_lines_reject_large_records_before_collecting_them() {
    let bytes = vec![b'x'; 1_000_000];
    let mut reader = BufReader::with_capacity(8, bytes.as_slice());
    let error = bounded_line(&mut reader, 32).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    // Only the bounded prefix has been consumed, rather than the entire record.
    assert_eq!(reader.into_inner().len(), 1_000_000 - 40);
}
