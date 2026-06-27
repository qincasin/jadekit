//! 手写 Protobuf varint 编解码 + Antigravity 凭据消息构造。
//!
//! 对齐上游 Antigravity-Manager 4.2.7 的 utils/protobuf.rs。
//! 不依赖 prost/protobuf 库,纯手写 wire format。

use base64::{engine::general_purpose, Engine as _};

/// Protobuf Varint Encoding
pub fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    while value >= 0x80 {
        buf.push((value & 0x7F | 0x80) as u8);
        value >>= 7;
    }
    buf.push(value as u8);
    buf
}

/// Read Protobuf Varint
pub fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), String> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;

    loop {
        if pos >= data.len() {
            return Err("incomplete_data".to_string());
        }
        let byte = data[pos];
        result |= ((byte & 0x7F) as u64) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Ok((result, pos))
}

/// Skip Protobuf Field
pub fn skip_field(data: &[u8], offset: usize, wire_type: u8) -> Result<usize, String> {
    match wire_type {
        0 => {
            let (_, new_offset) = read_varint(data, offset)?;
            Ok(new_offset)
        }
        1 => Ok(offset + 8),
        2 => {
            let (length, content_offset) = read_varint(data, offset)?;
            Ok(content_offset + length as usize)
        }
        5 => Ok(offset + 4),
        _ => Err(format!("unknown_wire_type: {}", wire_type)),
    }
}

/// Find specified Protobuf field content (Length-Delimited only)
#[allow(dead_code)]
pub fn find_field(data: &[u8], target_field: u32) -> Result<Option<Vec<u8>>, String> {
    let mut offset = 0;

    while offset < data.len() {
        let (tag, new_offset) = match read_varint(data, offset) {
            Ok(v) => v,
            Err(_) => break,
        };

        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset)?;
            return Ok(Some(
                data[content_offset..content_offset + length as usize].to_vec(),
            ));
        }

        offset = skip_field(data, new_offset, wire_type)?;
    }

    Ok(None)
}

/// 编码长度分隔字段 (wire_type = 2)
pub fn encode_len_delim_field(field_num: u32, data: &[u8]) -> Vec<u8> {
    let tag = (field_num << 3) | 2;
    let mut f = encode_varint(tag as u64);
    f.extend(encode_varint(data.len() as u64));
    f.extend_from_slice(data);
    f
}

/// 编码字符串字段 (wire_type = 2)
pub fn encode_string_field(field_num: u32, value: &str) -> Vec<u8> {
    encode_len_delim_field(field_num, value.as_bytes())
}

/// 编码 varint 字段 (wire_type = 0)
pub fn encode_varint_field(field_num: u32, value: u64) -> Vec<u8> {
    let tag = (field_num << 3) | 0;
    let mut f = encode_varint(tag as u64);
    f.extend(encode_varint(value));
    f
}

/// 创建 OAuthTokenInfo 消息(对齐上游 create_oauth_info)
pub fn create_oauth_info(
    access_token: &str,
    refresh_token: &str,
    expiry: i64,
    mut is_gcp_tos: bool,
    id_token: Option<&str>,
    email: Option<&str>,
) -> Vec<u8> {
    if let Some(email_str) = email {
        let lower = email_str.to_lowercase();
        let is_personal = lower.ends_with("@gmail.com")
            || lower.ends_with("@outlook.com")
            || lower.ends_with("@hotmail.com")
            || lower.ends_with("@qq.com")
            || lower.ends_with("@163.com");

        if is_personal && is_gcp_tos {
            tracing::info!(
                "[Protobuf] 自动纠正个人账号 ({}) 的 GCP 标志位以确保 IDE 刷新兼容性。",
                email_str
            );
            is_gcp_tos = false;
        }
    }

    let field1 = encode_string_field(1, access_token);
    let field2 = encode_string_field(2, "Bearer");
    let field3 = encode_string_field(3, refresh_token);

    let seconds_tag = (1 << 3) | 0;
    let mut timestamp_msg = encode_varint(seconds_tag);
    timestamp_msg.extend(encode_varint(expiry as u64));
    let nanos_tag = (2 << 3) | 0;
    timestamp_msg.extend(encode_varint(nanos_tag));
    timestamp_msg.extend(encode_varint(0));
    let field4 = encode_len_delim_field(4, &timestamp_msg);

    let field5 = id_token.map(|it| encode_string_field(5, it));
    let field6 = is_gcp_tos.then(|| encode_varint_field(6, 1));

    let mut oauth_info = Vec::new();
    oauth_info.extend(field1);
    oauth_info.extend(field2);
    oauth_info.extend(field3);
    oauth_info.extend(field4);
    if let Some(f) = field5 {
        oauth_info.extend(f);
    }
    if let Some(f) = field6 {
        oauth_info.extend(f);
    }
    oauth_info
}

/// 创建 unified-state stringValue payload
pub fn create_string_value_payload(value: &str) -> Vec<u8> {
    encode_string_field(3, value)
}

/// 创建最小可用的 UserStatus payload。
pub fn create_minimal_user_status_payload(email: &str) -> Vec<u8> {
    [encode_string_field(3, email), encode_string_field(7, email)].concat()
}

/// 创建 unified-state Topic.data entry。
pub fn create_unified_topic_entry(sentinel_key: &str, payload: &[u8]) -> Vec<u8> {
    let row = encode_string_field(1, &general_purpose::STANDARD.encode(payload));
    let entry = [
        encode_string_field(1, sentinel_key),
        encode_len_delim_field(2, &row),
    ]
    .concat();
    encode_len_delim_field(1, &entry)
}

/// 从 Topic.data 中移除指定 map entry,保留同 topic 下其他 sentinel row。
pub fn remove_unified_topic_entry(data: &[u8], target_key: &str) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut offset = 0;

    while offset < data.len() {
        let start_offset = offset;
        let (tag, new_offset) = read_varint(data, offset)?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;
        let next_offset = skip_field(data, new_offset, wire_type)?;

        let should_remove = if field_num == 1 && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset)?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return Err("Topic.data entry 数据不完整".to_string());
            }
            let entry = &data[content_offset..content_offset + length];
            unified_topic_entry_key(entry) == Some(target_key)
        } else {
            false
        };

        if !should_remove {
            result.extend_from_slice(&data[start_offset..next_offset]);
        }
        offset = next_offset;
    }

    Ok(result)
}

fn unified_topic_entry_key(data: &[u8]) -> Option<&str> {
    let mut offset = 0;
    while offset < data.len() {
        let (tag, new_offset) = read_varint(data, offset).ok()?;
        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == 1 && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset).ok()?;
            let length = length as usize;
            if content_offset + length > data.len() {
                return None;
            }
            return std::str::from_utf8(&data[content_offset..content_offset + length]).ok();
        }

        offset = skip_field(data, new_offset, wire_type).ok()?;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_varint_roundtrip() {
        for v in [0u64, 1, 127, 128, 16384, 0xFFFF_FFFF, u64::MAX] {
            let encoded = encode_varint(v);
            let (decoded, _) = read_varint(&encoded, 0).unwrap();
            assert_eq!(decoded, v, "varint roundtrip failed for {}", v);
        }
    }

    #[test]
    fn create_oauth_info_contains_all_fields() {
        let info = create_oauth_info("at", "rt", 1_700_000_000, false, None, Some("a@gmail.com"));
        let f1 = find_field(&info, 1).unwrap().unwrap();
        assert_eq!(String::from_utf8(f1).unwrap(), "at");
        let f2 = find_field(&info, 2).unwrap().unwrap();
        assert_eq!(String::from_utf8(f2).unwrap(), "Bearer");
        let f3 = find_field(&info, 3).unwrap().unwrap();
        assert_eq!(String::from_utf8(f3).unwrap(), "rt");
        // 个人账号 is_gcp_tos 应被强制关 → field 6 不存在
        assert!(find_field(&info, 6).unwrap().is_none());
    }

    #[test]
    fn unified_topic_entry_removes_and_adds() {
        let payload_a = create_oauth_info("at_a", "rt", 1, false, None, None);
        let payload_b = create_oauth_info("at_b", "rt", 2, false, None, None);
        let mut topic = create_unified_topic_entry("oauthTokenInfoSentinelKey", &payload_a);
        topic.extend(create_unified_topic_entry("otherKey", &payload_b));

        let cleaned = remove_unified_topic_entry(&topic, "oauthTokenInfoSentinelKey").unwrap();
        assert!(!cleaned.is_empty());
        assert!(cleaned.len() < topic.len(), "removing one entry shrinks topic");
    }
}
