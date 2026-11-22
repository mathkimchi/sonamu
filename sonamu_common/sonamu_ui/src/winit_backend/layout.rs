//! Where monitors go in the scene (#158), like niri: by the `displays` setting, matched by connector or
//! `"Make Model Serial"`; the rest, and overlapping ones, to the right of the others

/// A connected monitor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    /// Eg `DP-1`
    pub connector: String,
    /// `"Make Model Serial"` from its EDID (`Unknown` for missing parts)
    pub description: String,
    /// `[width, height]`, px
    pub size: [u32; 2],
}

/// A monitor's entry in the `displays` setting
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Placement {
    /// Its top left, px; `None`: to the right of the others
    pub position: Option<[i32; 2]>,
    /// The main screen: the wl output (layer surfaces, capture). The first monitor if none is.
    pub primary: bool,
}

/// Monitor vendors by PNP id (hwdata's `pnp.ids`: `DEL\tDell Inc.` lines, sorted)
const PNP_IDS: &str = include_str!("../../../../assets/pnp.ids");

/// `"Make Model Serial"` from an EDID like libdisplay-info (so like niri), `Unknown` for what it lacks:
/// the vendor's name (else its PNP id), the EDID's name and serial text, else their numbers in hex.
#[must_use]
pub fn description(edid: &[u8]) -> String {
    const HEADER: [u8; 8] = [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0];
    let edid = edid
        .get(..128)
        .filter(|edid| edid.starts_with(&HEADER))
        .unwrap_or_default();
    // the display descriptors' text by tag (eg 0xFC: name)
    let text = |tag: u8| {
        edid.get(54..126)?
            .as_chunks::<18>()
            .0
            .iter()
            .find_map(|descriptor| {
                (descriptor[..3] == [0, 0, 0] && descriptor[3] == tag).then(|| {
                    let text = &descriptor[5..];
                    let end = text.iter().position(|&byte| byte == b'\n');
                    String::from_utf8_lossy(&text[..end.unwrap_or(text.len())])
                        .trim()
                        .to_string()
                })
            })
    };
    let make = edid.get(8..10).and_then(|id| {
        let id = u16::from_be_bytes([id[0], id[1]]);
        [10, 5, 0]
            .into_iter()
            .map(|shift| match (id >> shift) & 0x1F {
                letter @ 1..=26 => Some(char::from(b'A' + letter as u8 - 1)),
                _ => None,
            })
            .collect::<Option<String>>()
            .map(|id| vendor(&id).map_or(id, str::to_string))
    });
    let model = text(0xFC).or_else(|| {
        let code = edid.get(10..12)?;
        Some(format!("0x{:04X}", u16::from_le_bytes([code[0], code[1]])))
    });
    let serial = text(0xFF).or_else(|| {
        let serial = u32::from_le_bytes(edid.get(12..16)?.try_into().ok()?);
        (serial != 0).then(|| format!("0x{serial:08X}"))
    });
    [make, model, serial]
        .map(|part| {
            part.filter(|part| !part.is_empty())
                .unwrap_or_else(|| "Unknown".to_string())
        })
        .join(" ")
}

/// The vendor's name for a PNP id (eg `DEL`)
fn vendor(id: &str) -> Option<&'static str> {
    PNP_IDS
        .lines()
        .find_map(|line| line.strip_prefix(id)?.strip_prefix('\t'))
}

/// Each monitor's name: its description, or its connector if another monitor has the same description
#[must_use]
pub fn names(monitors: &[Monitor]) -> Vec<String> {
    monitors
        .iter()
        .map(|monitor| {
            let shared = monitors
                .iter()
                .filter(|other| other.description == monitor.description)
                .count()
                > 1;
            if shared {
                monitor.connector.clone()
            } else {
                monitor.description.clone()
            }
        })
        .collect()
}

/// The monitors (indices into `monitors`), primary first, then in their order, and each one's top left:
/// placed by `layout` (keys matched without case by name, else connector), the rest to the right of the others.
/// Moved so the top left is `[0, 0]`.
#[must_use]
pub fn arrange(monitors: &[Monitor], layout: &[(String, Placement)]) -> Vec<(usize, [u32; 2])> {
    let names = names(monitors);
    let placement = |index: usize| {
        let monitor: &Monitor = &monitors[index];
        [&names[index], &monitor.connector]
            .into_iter()
            .find_map(|name| {
                layout
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, placement)| *placement)
            })
            .unwrap_or_default()
    };
    let primary = (0..monitors.len())
        .find(|&index| placement(index).primary)
        .unwrap_or(0);
    let order: Vec<usize> = std::iter::once(primary)
        .chain((0..monitors.len()).filter(|&index| index != primary))
        .take(monitors.len())
        .collect();

    let rect = |index: usize, [x, y]: [i32; 2]| {
        let [width, height] = monitors[index].size.map(u32::cast_signed);
        [x, y, x + width, y + height]
    };
    let mut placed: Vec<(usize, [i32; 4])> = Vec::new();
    for &index in &order {
        if let Some(position) = placement(index).position {
            let [left, top, right, bottom] = rect(index, position);
            let overlaps = placed
                .iter()
                .any(|(_, [l, t, r, b])| left < *r && *l < right && top < *b && *t < bottom);
            if !overlaps {
                placed.push((index, [left, top, right, bottom]));
            }
        }
    }
    for &index in &order {
        if !placed.iter().any(|&(placed, _)| placed == index) {
            let right = placed.iter().map(|(_, [.., r, _])| *r).max().unwrap_or(0);
            placed.push((index, rect(index, [right, 0])));
        }
    }
    let left = placed.iter().map(|(_, [l, ..])| *l).min().unwrap_or(0);
    let top = placed.iter().map(|(_, [_, t, ..])| *t).min().unwrap_or(0);
    order
        .iter()
        .map(|&index| {
            let [x, y, ..] = placed.iter().find(|&&(i, _)| i == index).unwrap().1;
            (
                index,
                [(x - left).cast_unsigned(), (y - top).cast_unsigned()],
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(connector: &str, description: &str, size: [u32; 2]) -> Monitor {
        Monitor {
            connector: connector.to_string(),
            description: description.to_string(),
            size,
        }
    }

    fn at(position: [i32; 2]) -> Placement {
        Placement {
            position: Some(position),
            primary: false,
        }
    }

    #[test]
    fn unplaced_side_by_side() {
        let monitors = [
            monitor("eDP-1", "BOE 0x0BCA Unknown", [1920, 1200]),
            monitor("DP-1", "DEL U2720Q ABC", [3840, 2160]),
        ];
        assert_eq!(arrange(&monitors, &[]), [(0, [0, 0]), (1, [1920, 0])]);
    }

    #[test]
    fn placed_by_name_or_connector_and_moved_to_origin() {
        let monitors = [
            monitor("eDP-1", "BOE 0x0BCA Unknown", [1920, 1200]),
            monitor("DP-1", "DEL U2720Q ABC", [3840, 2160]),
        ];
        let layout = [
            ("del u2720q abc".to_string(), at([0, -2160])),
            ("eDP-1".to_string(), at([960, 0])),
        ];
        assert_eq!(arrange(&monitors, &layout), [(0, [960, 2160]), (1, [0, 0])]);
    }

    #[test]
    fn primary_first_and_overlap_moved_right() {
        let monitors = [
            monitor("eDP-1", "A", [1000, 800]),
            monitor("DP-1", "B", [1000, 800]),
            monitor("DP-2", "C", [500, 500]),
        ];
        let layout = [
            (
                "B".to_string(),
                Placement {
                    position: Some([0, 0]),
                    primary: true,
                },
            ),
            ("A".to_string(), at([500, 0])),
        ];
        assert_eq!(
            arrange(&monitors, &layout),
            [(1, [0, 0]), (0, [1000, 0]), (2, [2000, 0])]
        );
    }

    #[test]
    fn description_from_edid() {
        let mut edid = [0_u8; 128];
        edid[..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        // "DEL": 4, 5, 12
        edid[8..10].copy_from_slice(&((4_u16 << 10) | (5 << 5) | 0xC).to_be_bytes());
        edid[10..12].copy_from_slice(&0xA0B1_u16.to_le_bytes());
        // a name descriptor, the others unused
        edid[54 + 3] = 0xFC;
        edid[54 + 5..54 + 18].copy_from_slice(b"DELL U2720Q\n ");
        for descriptor in 1..4 {
            edid[54 + 18 * descriptor + 3] = 0x10;
        }
        assert_eq!(description(&edid), "Dell Inc. DELL U2720Q Unknown");
        edid[12..16].copy_from_slice(&1234_u32.to_le_bytes());
        edid[54 + 3] = 0x10;
        assert_eq!(description(&edid), "Dell Inc. 0xA0B1 0x000004D2");
        assert_eq!(description(&[]), "Unknown Unknown Unknown");
    }

    #[test]
    fn same_description_named_by_connector() {
        let monitors = [
            monitor("DP-1", "DEL P2419H Unknown", [1920, 1080]),
            monitor("DP-2", "DEL P2419H Unknown", [1920, 1080]),
            monitor("eDP-1", "BOE X Unknown", [1920, 1080]),
        ];
        assert_eq!(names(&monitors), ["DP-1", "DP-2", "BOE X Unknown"]);
    }
}
