//! Validate binary ranges independently of preview or export selection.
use super::GlbError;
use gltf::{accessor::DataType, buffer::Source};

fn invalid(message: &str) -> GlbError {
    GlbError::Invalid(message.into())
}

fn range(
    view: &gltf::buffer::View<'_>,
    offset: usize,
    count: usize,
    stride: usize,
    size: usize,
) -> Result<usize, GlbError> {
    let length = count
        .saturating_sub(1)
        .checked_mul(stride)
        .and_then(|v| v.checked_add(if count == 0 { 0 } else { size }))
        .and_then(|v| v.checked_add(offset))
        .ok_or_else(|| invalid("Accessor range overflows"))?;
    if length > view.length() {
        return Err(invalid("Accessor range exceeds its bufferView"));
    }
    view.offset()
        .checked_add(offset)
        .ok_or_else(|| invalid("Accessor offset overflows"))
}

fn finite_values(
    view: &gltf::buffer::View<'_>,
    start: usize,
    count: usize,
    stride: usize,
    components: usize,
    bin: Option<&[u8]>,
) -> Result<(), GlbError> {
    if !matches!(view.buffer().source(), Source::Bin) {
        return Ok(());
    }
    let bytes = bin.ok_or_else(|| invalid("Missing GLB binary buffer"))?;
    for index in 0..count {
        let offset = start + index * stride;
        for component in 0..components {
            let offset = offset + component * 4;
            let value = bytes.get(offset..offset + 4).ok_or_else(|| {
                invalid("Accessor range exceeds binary buffer")
            })?;
            let value =
                f32::from_le_bytes([value[0], value[1], value[2], value[3]]);
            if !value.is_finite() {
                return Err(invalid(
                    "Accessor contains non-finite float values",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate(
    document: &gltf::Document,
    bin: Option<&[u8]>,
) -> Result<(), GlbError> {
    for buffer in document.buffers() {
        if matches!(buffer.source(), Source::Bin)
            && buffer.length() > bin.map_or(0, |v| v.len())
        {
            return Err(invalid(
                "Declared GLB buffer length exceeds binary chunk",
            ));
        }
    }
    for view in document.views() {
        let end = view
            .offset()
            .checked_add(view.length())
            .ok_or_else(|| invalid("BufferView range overflows"))?;
        if end > view.buffer().length() {
            return Err(invalid("BufferView range exceeds its buffer"));
        }
    }
    for accessor in document.accessors() {
        let component_size = accessor.data_type().size();
        let dimensions = accessor.dimensions();
        let components = dimensions.multiplicity();
        // Matrix columns containing small integer components are padded to four bytes.
        let size = match dimensions {
            gltf::accessor::Dimensions::Mat2 => {
                (component_size * 2).div_ceil(4) * 4 * 2
            }
            gltf::accessor::Dimensions::Mat3 => {
                (component_size * 3).div_ceil(4) * 4 * 3
            }
            _ => component_size * components,
        };
        if let Some(view) = accessor.view() {
            let stride = view.stride().unwrap_or(size);
            if stride < size
                || stride % component_size != 0
                || view
                    .offset()
                    .checked_add(accessor.offset())
                    .is_none_or(|v| v % component_size != 0)
            {
                return Err(invalid("Accessor stride or alignment is invalid"));
            }
            let start = range(
                &view,
                accessor.offset(),
                accessor.count(),
                stride,
                size,
            )?;
            if accessor.data_type() == DataType::F32 {
                finite_values(
                    &view,
                    start,
                    accessor.count(),
                    stride,
                    components,
                    bin,
                )?;
            }
        }
        if let Some(sparse) = accessor.sparse() {
            if sparse.count() > accessor.count() {
                return Err(invalid(
                    "Sparse accessor count exceeds accessor count",
                ));
            }
            let indices = sparse.indices();
            let view = indices.view();
            let index_size = indices.index_type().size();
            if view
                .offset()
                .checked_add(indices.offset())
                .is_none_or(|v| v % index_size != 0)
            {
                return Err(invalid("Sparse indices alignment is invalid"));
            }
            let start = range(
                &view,
                indices.offset(),
                sparse.count(),
                index_size,
                index_size,
            )?;
            if matches!(view.buffer().source(), Source::Bin) {
                let bytes =
                    bin.ok_or_else(|| invalid("Missing GLB binary buffer"))?;
                let mut previous = None;
                for i in 0..sparse.count() {
                    let offset = start + i * index_size;
                    let data = bytes
                        .get(offset..offset + index_size)
                        .ok_or_else(|| {
                            invalid(
                                "Sparse indices range exceeds binary buffer",
                            )
                        })?;
                    let index = match index_size {
                        1 => usize::from(data[0]),
                        2 => {
                            usize::from(u16::from_le_bytes([data[0], data[1]]))
                        }
                        4 => u32::from_le_bytes([
                            data[0], data[1], data[2], data[3],
                        ]) as usize,
                        _ => {
                            return Err(invalid(
                                "Invalid sparse index component size",
                            ))
                        }
                    };
                    if index >= accessor.count()
                        || previous.is_some_and(|v| v >= index)
                    {
                        return Err(invalid("Sparse indices must be ordered, unique, and within the accessor"));
                    }
                    previous = Some(index);
                }
            }
            let values = sparse.values();
            let view = values.view();
            if view
                .offset()
                .checked_add(values.offset())
                .is_none_or(|v| v % component_size != 0)
            {
                return Err(invalid("Sparse values alignment is invalid"));
            }
            let start =
                range(&view, values.offset(), sparse.count(), size, size)?;
            if accessor.data_type() == DataType::F32 {
                finite_values(
                    &view,
                    start,
                    sparse.count(),
                    size,
                    components,
                    bin,
                )?;
            }
        }
    }
    Ok(())
}
