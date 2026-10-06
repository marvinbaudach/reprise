use super::startup_tasks::SignatureTask;

#[test]
fn spectrogram_completion_uses_the_loudness_revision_key() {
    assert_eq!(
        SignatureTask::Spectrogram.key(),
        "startup_tasks.completed.spectrogram-v2"
    );
}
