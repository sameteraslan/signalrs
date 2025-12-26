//! Subject-like streaming API for incremental push-based streaming
//!
//! This module provides a `StreamSubject<T>` type that enables a Python Subject-like
//! pattern for client-to-server streaming. Instead of creating an entire stream upfront,
//! you can create a subject, invoke a method with it, and then push items incrementally.
//!
//! # Example
//! ```rust,no_run
//! use signalrs_client::{SignalRClient, subject::StreamSubject};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let client = SignalRClient::builder("localhost")
//! #     .build()
//! #     .await?;
//! // Create a subject
//! let subject = StreamSubject::new();
//! let pusher = subject.clone();
//!
//! // Start the invocation with the stream
//! client
//!     .method("Upload")
//!     .arg(subject.stream())?
//!     .send()
//!     .await?;
//!
//! // Later, push items incrementally
//! pusher.push(1).await?;
//! pusher.push(2).await?;
//! pusher.push(3).await?;
//! pusher.complete();
//! # Ok(())
//! # }
//! ```

use crate::arguments::InvocationStream;
use flume::{Receiver, Sender};
use thiserror::Error;

/// Error type for StreamSubject operations
#[derive(Debug, Error)]
pub enum SubjectError {
    /// The subject stream has been closed and can no longer accept items
    #[error("Subject stream has been closed")]
    Closed,
}

/// A Subject-like type for creating streams that can be pushed to incrementally
///
/// Similar to Python's Subject pattern, this allows you to create a stream
/// and push items to it over time, rather than creating the entire stream upfront.
///
/// # Design
///
/// - Uses an unbounded channel internally to avoid backpressure deadlocks
/// - `stream()` consumes the subject and returns the receiver as an `InvocationStream`
/// - `push()` uses `&self` so the subject can be cloned and shared across threads
/// - `complete()` consumes the subject to ensure no more items can be pushed
/// - When all senders are dropped, the receiver stream ends naturally
/// - The `AppendCompletion` wrapper automatically sends a SignalR Completion message
///
/// # Example
/// ```rust,no_run
/// use signalrs_client::{SignalRClient, subject::StreamSubject};
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let client = SignalRClient::builder("localhost")
/// #     .build()
/// #     .await?;
/// let subject = StreamSubject::new();
///
/// // Clone for use in async tasks
/// let pusher = subject.clone();
///
/// // Start the invocation
/// client.method("Upload")
///     .arg(subject.stream())?
///     .send()
///     .await?;
///
/// // Push from another task
/// tokio::spawn(async move {
///     for i in 1..=5 {
///         pusher.push(i).await.unwrap();
///     }
///     pusher.complete();
/// });
/// # Ok(())
/// # }
/// ```
pub struct StreamSubject<T> {
    tx: Sender<T>,
    rx: Option<Receiver<T>>,
}

impl<T> StreamSubject<T> {
    /// Creates a new StreamSubject with an unbounded channel
    ///
    /// # Example
    /// ```rust
    /// use signalrs_client::subject::StreamSubject;
    ///
    /// let subject = StreamSubject::<i32>::new();
    /// ```
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded();
        StreamSubject {
            tx,
            rx: Some(rx),
        }
    }

    /// Pushes an item to the stream asynchronously
    ///
    /// Returns an error if the stream has been closed (either by calling `complete()`
    /// or by converting to an `InvocationStream` via `stream()`).
    ///
    /// # Example
    /// ```rust
    /// use signalrs_client::subject::StreamSubject;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let subject = StreamSubject::new();
    /// subject.push(42).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn push(&self, item: T) -> Result<(), SubjectError> {
        self.tx
            .send_async(item)
            .await
            .map_err(|_| SubjectError::Closed)
    }

    /// Marks the stream as complete by closing the sender
    ///
    /// This consumes the subject, ensuring no more items can be pushed.
    /// The receiver will naturally end, triggering the Completion message.
    ///
    /// Note: If you don't call `complete()`, the stream will close when all
    /// clones of the subject are dropped.
    ///
    /// # Example
    /// ```rust
    /// use signalrs_client::subject::StreamSubject;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let subject = StreamSubject::new();
    /// subject.push(1).await?;
    /// subject.push(2).await?;
    /// subject.complete(); // Signal completion
    /// # Ok(())
    /// # }
    /// ```
    pub fn complete(self) {
        drop(self.tx);
    }

    /// Converts the subject into an InvocationStream for use with `.arg()`
    ///
    /// This consumes the subject and takes ownership of the receiver.
    /// After calling this, you can only push items using clones of the subject
    /// created before this call.
    ///
    /// # Panics
    ///
    /// Panics if `stream()` is called twice on the same subject.
    ///
    /// # Example
    /// ```rust,no_run
    /// use signalrs_client::{SignalRClient, subject::StreamSubject};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = SignalRClient::builder("localhost")
    /// #     .build()
    /// #     .await?;
    /// let subject = StreamSubject::new();
    /// let pusher = subject.clone();
    ///
    /// // Convert to InvocationStream and use with .arg()
    /// client.method("Upload")
    ///     .arg(subject.stream())?
    ///     .send()
    ///     .await?;
    ///
    /// // Can still push using the clone
    /// pusher.push(42).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn stream(mut self) -> InvocationStream<T>
    where
        T: Send + 'static,
    {
        let rx = self
            .rx
            .take()
            .expect("stream() called twice on the same StreamSubject");
        InvocationStream::new(rx.into_stream())
    }
}

impl<T> Clone for StreamSubject<T> {
    /// Creates a clone of the subject that shares the same underlying channel
    ///
    /// Clones only get the sender, not the receiver. This allows multiple
    /// threads to push to the same stream.
    ///
    /// # Example
    /// ```rust
    /// use signalrs_client::subject::StreamSubject;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let subject = StreamSubject::new();
    /// let clone1 = subject.clone();
    /// let clone2 = subject.clone();
    ///
    /// // All three can push to the same stream
    /// subject.push(1).await?;
    /// clone1.push(2).await?;
    /// clone2.push(3).await?;
    /// # Ok(())
    /// # }
    /// ```
    fn clone(&self) -> Self {
        StreamSubject {
            tx: self.tx.clone(),
            rx: None, // Only the original has the receiver
        }
    }
}

impl<T> Default for StreamSubject<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_basic_push_and_complete() {
        let subject = StreamSubject::new();
        let pusher = subject.clone();

        tokio::spawn(async move {
            pusher.push(1).await.unwrap();
            pusher.push(2).await.unwrap();
            pusher.push(3).await.unwrap();
            pusher.complete();
        });

        let mut stream = subject.stream();
        let mut items = Vec::new();
        while let Some(item) = stream.next().await {
            items.push(item);
        }

        assert_eq!(items, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_push_after_complete_fails() {
        let subject = StreamSubject::new();
        let pusher = subject.clone();

        subject.complete();

        let result = pusher.push(1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiple_senders() {
        let subject = StreamSubject::new();
        let pusher1 = subject.clone();
        let pusher2 = subject.clone();

        tokio::spawn(async move {
            pusher1.push(1).await.unwrap();
            pusher1.push(2).await.unwrap();
        });

        tokio::spawn(async move {
            pusher2.push(3).await.unwrap();
            pusher2.push(4).await.unwrap();
        });

        // Give spawned tasks time to push
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Drop all senders to complete the stream
        drop(subject);

        // Note: We can't easily test the receiver here since we dropped the original subject
        // This test mainly verifies that multiple senders work without panicking
    }

    #[tokio::test]
    #[should_panic(expected = "stream() called twice")]
    async fn test_stream_on_clone_panics() {
        let subject = StreamSubject::<i32>::new();
        let clone = subject.clone();
        // Clones don't have the receiver, so calling stream() should panic
        let _stream = clone.stream();
    }
}
