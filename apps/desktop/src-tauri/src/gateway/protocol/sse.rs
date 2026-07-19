use serde_json::Value;

#[derive(Default)]
pub(super) struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, String> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some((end, delimiter_len)) = next_frame(&self.buffer) {
            let frame = self.buffer.drain(..end + delimiter_len).collect::<Vec<_>>();
            if let Some(data) = frame_data(&frame[..end])? {
                events.push(data);
            }
        }
        Ok(events)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<String>, String> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let remaining = std::mem::take(&mut self.buffer);
        Ok(frame_data(&remaining)?.into_iter().collect())
    }
}

pub(super) fn push_event(output: &mut String, event: &str, value: &Value) {
    output.push_str("event: ");
    output.push_str(event);
    output.push_str("\ndata: ");
    output.push_str(&serde_json::to_string(value).expect("JSON values always serialize"));
    output.push_str("\n\n");
}

pub(super) fn push_data(output: &mut String, value: &Value) {
    output.push_str("data: ");
    output.push_str(&serde_json::to_string(value).expect("JSON values always serialize"));
    output.push_str("\n\n");
}

pub(super) fn push_done(output: &mut String) {
    output.push_str("data: [DONE]\n\n");
}

fn next_frame(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|index| (index, 2));
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn frame_data(frame: &[u8]) -> Result<Option<String>, String> {
    let frame = std::str::from_utf8(frame).map_err(|error| error.to_string())?;
    let data = frame
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>()
        .join("\n");
    Ok((!data.is_empty()).then_some(data))
}
