use super::{
    stream_anthropic::AnthropicStreamConverter, stream_openai::OpenAiStreamConverter, GatewayFormat,
};

pub(in crate::gateway) struct GatewayStreamConverter {
    pipeline: StreamPipeline,
}

enum StreamPipeline {
    Single(StreamStage),
    Chained(StreamStage, StreamStage),
}

enum StreamStage {
    OpenAi(OpenAiStreamConverter),
    Anthropic(AnthropicStreamConverter),
}

impl GatewayStreamConverter {
    pub(in crate::gateway) fn new(from: GatewayFormat, to: GatewayFormat) -> Result<Self, String> {
        let pipeline = match (from, to) {
            (GatewayFormat::OpenAiCompatible, GatewayFormat::OpenAiResponses)
            | (GatewayFormat::OpenAiResponses, GatewayFormat::OpenAiCompatible) => {
                StreamPipeline::Single(StreamStage::OpenAi(OpenAiStreamConverter::new(from, to)?))
            }
            (GatewayFormat::OpenAiCompatible, GatewayFormat::Anthropic)
            | (GatewayFormat::Anthropic, GatewayFormat::OpenAiCompatible) => {
                StreamPipeline::Single(StreamStage::Anthropic(AnthropicStreamConverter::new(
                    from, to,
                )?))
            }
            (GatewayFormat::OpenAiResponses, GatewayFormat::Anthropic) => StreamPipeline::Chained(
                StreamStage::OpenAi(OpenAiStreamConverter::new(
                    GatewayFormat::OpenAiResponses,
                    GatewayFormat::OpenAiCompatible,
                )?),
                StreamStage::Anthropic(AnthropicStreamConverter::new(
                    GatewayFormat::OpenAiCompatible,
                    GatewayFormat::Anthropic,
                )?),
            ),
            (GatewayFormat::Anthropic, GatewayFormat::OpenAiResponses) => StreamPipeline::Chained(
                StreamStage::Anthropic(AnthropicStreamConverter::new(
                    GatewayFormat::Anthropic,
                    GatewayFormat::OpenAiCompatible,
                )?),
                StreamStage::OpenAi(OpenAiStreamConverter::new(
                    GatewayFormat::OpenAiCompatible,
                    GatewayFormat::OpenAiResponses,
                )?),
            ),
            _ => return Err("Gateway stream converter received identical formats".to_string()),
        };
        Ok(Self { pipeline })
    }

    pub(in crate::gateway) fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, String> {
        match &mut self.pipeline {
            StreamPipeline::Single(stage) => stage.push(chunk),
            StreamPipeline::Chained(first, second) => {
                let intermediate = first.push(chunk)?;
                second.push(&intermediate)
            }
        }
    }

    pub(in crate::gateway) fn finish(&mut self) -> Result<Vec<u8>, String> {
        match &mut self.pipeline {
            StreamPipeline::Single(stage) => stage.finish(),
            StreamPipeline::Chained(first, second) => {
                let intermediate = first.finish()?;
                let mut output = second.push(&intermediate)?;
                output.extend(second.finish()?);
                Ok(output)
            }
        }
    }
}

impl StreamStage {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, String> {
        match self {
            Self::OpenAi(converter) => converter.push(chunk),
            Self::Anthropic(converter) => converter.push(chunk),
        }
    }

    fn finish(&mut self) -> Result<Vec<u8>, String> {
        match self {
            Self::OpenAi(converter) => converter.finish(),
            Self::Anthropic(converter) => converter.finish(),
        }
    }
}

#[cfg(test)]
pub(in crate::gateway) fn convert_stream_body(
    body: &[u8],
    from: GatewayFormat,
    to: GatewayFormat,
) -> Result<Vec<u8>, String> {
    if from == to {
        return Ok(body.to_vec());
    }
    let mut converter = GatewayStreamConverter::new(from, to)?;
    let mut output = converter.push(body)?;
    output.extend(converter.finish()?);
    Ok(output)
}
