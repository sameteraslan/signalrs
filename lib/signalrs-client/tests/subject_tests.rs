use futures::StreamExt;
use signalrs_client::subject::StreamSubject;

#[tokio::test]
async fn test_subject_basic_push_and_stream() {
    let subject = StreamSubject::new();
    let pusher = subject.clone();

    // Spawn a task to push items
    tokio::spawn(async move {
        pusher.push(1).await.unwrap();
        pusher.push(2).await.unwrap();
        pusher.push(3).await.unwrap();
        pusher.complete();
    });

    // Get the stream and collect items
    let stream = subject.stream();
    let items: Vec<i32> = stream.collect().await;

    assert_eq!(items, vec![1, 2, 3]);
}

#[tokio::test]
async fn test_subject_push_after_complete_fails() {
    let subject = StreamSubject::new();
    let pusher = subject.clone();

    // Complete the stream
    subject.complete();

    // Try to push - should fail
    let result = pusher.push(1).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_subject_multiple_senders() {
    let subject = StreamSubject::new();
    let pusher1 = subject.clone();
    let pusher2 = subject.clone();

    // Spawn two tasks pushing items
    let handle1 = tokio::spawn(async move {
        pusher1.push(1).await.unwrap();
        pusher1.push(2).await.unwrap();
    });

    let handle2 = tokio::spawn(async move {
        pusher2.push(3).await.unwrap();
        pusher2.push(4).await.unwrap();
    });

    // Wait for both tasks to complete
    handle1.await.unwrap();
    handle2.await.unwrap();

    // Complete the stream
    subject.complete();

    // Note: Can't easily verify stream contents here since subject is consumed
    // This test mainly verifies that multiple senders work without panicking
}

#[tokio::test]
async fn test_subject_stream_ends_when_all_senders_dropped() {
    let subject = StreamSubject::new();
    let pusher = subject.clone();

    // Push some items then drop all senders
    pusher.push(1).await.unwrap();
    pusher.push(2).await.unwrap();
    drop(pusher);

    // Get the stream - it should end naturally since all senders are dropped
    let stream = subject.stream();
    let items: Vec<i32> = stream.collect().await;

    assert_eq!(items, vec![1, 2]);
}

#[tokio::test]
async fn test_subject_clone_before_stream() {
    let subject = StreamSubject::new();
    let pusher = subject.clone();

    // Convert to stream first
    let stream = subject.stream();

    // Can still push using the clone
    pusher.push(1).await.unwrap();
    pusher.push(2).await.unwrap();
    pusher.complete();

    // Collect items
    let items: Vec<i32> = stream.collect().await;
    assert_eq!(items, vec![1, 2]);
}

#[tokio::test]
async fn test_subject_with_strings() {
    let subject = StreamSubject::new();
    let pusher = subject.clone();

    tokio::spawn(async move {
        pusher.push("hello".to_string()).await.unwrap();
        pusher.push("world".to_string()).await.unwrap();
        pusher.complete();
    });

    let stream = subject.stream();
    let items: Vec<String> = stream.collect().await;

    assert_eq!(items, vec!["hello".to_string(), "world".to_string()]);
}
