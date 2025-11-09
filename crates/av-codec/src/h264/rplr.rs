//! Reference Picture List Reordering (RPLR) for H.264
//!
//! ISO/IEC 14496-10:2022 §8.2.4.3 (Modification process for reference picture lists)
//!
//! RPLR allows the encoder to modify the default reference picture lists
//! constructed by the DPB to achieve optimal prediction efficiency by
//! reordering pictures based on temporal distance or importance.

use av_core::{Error, Result};

/// Reference picture list reordering command
///
/// ISO/IEC 14496-10:2022 §7.3.3.1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RplrCommand {
    /// Move short-term reference picture with pic_num - abs_diff_pic_num
    ShortTermSub { abs_diff_pic_num: u32 },
    /// Move short-term reference picture with pic_num + abs_diff_pic_num
    ShortTermAdd { abs_diff_pic_num: u32 },
    /// Move long-term reference picture with given index
    LongTerm { long_term_pic_num: u32 },
    /// End of modification commands
    End,
}

impl RplrCommand {
    /// Create from modification_of_pic_nums_idc and parameter
    ///
    /// ISO/IEC 14496-10:2022 Table 7-7
    pub fn from_idc(idc: u32, param: u32) -> Result<Self> {
        match idc {
            0 => Ok(Self::ShortTermSub {
                abs_diff_pic_num: param,
            }),
            1 => Ok(Self::ShortTermAdd {
                abs_diff_pic_num: param,
            }),
            2 => Ok(Self::LongTerm {
                long_term_pic_num: param,
            }),
            3 => Ok(Self::End),
            _ => Err(Error::invalid(
                "RPLR",
                &format!("Invalid modification_of_pic_nums_idc: {}", idc),
            )),
        }
    }

    /// Get modification_of_pic_nums_idc value
    pub fn to_idc(&self) -> u32 {
        match self {
            Self::ShortTermSub { .. } => 0,
            Self::ShortTermAdd { .. } => 1,
            Self::LongTerm { .. } => 2,
            Self::End => 3,
        }
    }
}

/// Reference picture list reordering state
///
/// ISO/IEC 14496-10:2022 §8.2.4.3
#[derive(Debug, Clone)]
pub struct RplrState {
    /// Commands to apply to List 0
    pub list0_commands: Vec<RplrCommand>,
    /// Commands to apply to List 1
    pub list1_commands: Vec<RplrCommand>,
    /// Is List 0 reordering present?
    pub list0_reordering: bool,
    /// Is List 1 reordering present?
    pub list1_reordering: bool,
}

impl RplrState {
    /// Create new RPLR state with no reordering
    pub fn new() -> Self {
        Self {
            list0_commands: Vec::new(),
            list1_commands: Vec::new(),
            list0_reordering: false,
            list1_reordering: false,
        }
    }

    /// Add reordering command for List 0
    pub fn add_list0_command(&mut self, cmd: RplrCommand) {
        self.list0_reordering = true;
        self.list0_commands.push(cmd);
    }

    /// Add reordering command for List 1
    pub fn add_list1_command(&mut self, cmd: RplrCommand) {
        self.list1_reordering = true;
        self.list1_commands.push(cmd);
    }
}

impl Default for RplrState {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference picture in a list (with pic_num)
#[derive(Debug, Clone)]
pub struct RefPicListEntry<'a, T> {
    /// Reference to the actual picture
    pub picture: &'a T,
    /// Picture number (for short-term refs)
    pub pic_num: i32,
    /// Long-term picture number (for long-term refs)
    pub long_term_pic_num: Option<u32>,
    /// Is this a long-term reference?
    pub is_long_term: bool,
}

/// Apply reference picture list reordering to List 0
///
/// ISO/IEC 14496-10:2022 §8.2.4.3.1
///
/// # Parameters
/// - `default_list`: Default reference picture list from DPB
/// - `commands`: Reordering commands from slice header
/// - `curr_pic_num`: Current picture number
/// - `max_pic_num`: MaxPicNum (depends on log2_max_frame_num)
///
/// # Returns
/// Reordered reference picture list
pub fn reorder_ref_pic_list<'a, T>(
    default_list: Vec<RefPicListEntry<'a, T>>,
    commands: &[RplrCommand],
    curr_pic_num: i32,
    max_pic_num: i32,
) -> Result<Vec<RefPicListEntry<'a, T>>> {
    if commands.is_empty() {
        return Ok(default_list);
    }

    let mut reordered_list = Vec::new();
    let mut ref_list = default_list;
    let mut reorder_idx = 0;
    let mut pic_num_pred = curr_pic_num;

    for cmd in commands {
        match cmd {
            RplrCommand::End => break,

            RplrCommand::ShortTermSub { abs_diff_pic_num } => {
                // pic_num_no_wrap = pic_num_pred - abs_diff_pic_num - 1
                let pic_num_no_wrap = pic_num_pred - (*abs_diff_pic_num as i32) - 1;

                // pic_num = (pic_num_no_wrap < 0) ? pic_num_no_wrap + max_pic_num : pic_num_no_wrap
                let pic_num = if pic_num_no_wrap < 0 {
                    pic_num_no_wrap + max_pic_num
                } else {
                    pic_num_no_wrap
                };

                // Find picture with this pic_num in ref_list
                let found_idx = ref_list
                    .iter()
                    .position(|e| !e.is_long_term && e.pic_num == pic_num)
                    .ok_or_else(|| {
                        Error::invalid("RPLR", &format!("Picture with pic_num {} not found", pic_num))
                    })?;

                // Move to reordered position
                let entry = ref_list.remove(found_idx);
                reordered_list.insert(reorder_idx, entry);
                reorder_idx += 1;

                pic_num_pred = pic_num;
            }

            RplrCommand::ShortTermAdd { abs_diff_pic_num } => {
                // pic_num_no_wrap = pic_num_pred + abs_diff_pic_num + 1
                let pic_num_no_wrap = pic_num_pred + (*abs_diff_pic_num as i32) + 1;

                // pic_num = (pic_num_no_wrap >= max_pic_num) ? pic_num_no_wrap - max_pic_num : pic_num_no_wrap
                let pic_num = if pic_num_no_wrap >= max_pic_num {
                    pic_num_no_wrap - max_pic_num
                } else {
                    pic_num_no_wrap
                };

                let found_idx = ref_list
                    .iter()
                    .position(|e| !e.is_long_term && e.pic_num == pic_num)
                    .ok_or_else(|| {
                        Error::invalid("RPLR", &format!("Picture with pic_num {} not found", pic_num))
                    })?;

                let entry = ref_list.remove(found_idx);
                reordered_list.insert(reorder_idx, entry);
                reorder_idx += 1;

                pic_num_pred = pic_num;
            }

            RplrCommand::LongTerm { long_term_pic_num } => {
                // Find long-term picture with this long_term_pic_num
                let found_idx = ref_list
                    .iter()
                    .position(|e| {
                        e.is_long_term && e.long_term_pic_num == Some(*long_term_pic_num)
                    })
                    .ok_or_else(|| {
                        Error::invalid(
                            "RPLR",
                            &format!("Long-term picture {} not found", long_term_pic_num),
                        )
                    })?;

                let entry = ref_list.remove(found_idx);
                reordered_list.insert(reorder_idx, entry);
                reorder_idx += 1;
            }
        }
    }

    // Append remaining pictures from ref_list
    reordered_list.extend(ref_list);

    Ok(reordered_list)
}

/// Calculate current picture number
///
/// ISO/IEC 14496-10:2022 §8.2.4.1
///
/// # Parameters
/// - `frame_num`: Current frame_num from slice header
/// - `max_frame_num`: MaxFrameNum (1 << log2_max_frame_num_minus4 + 4)
///
/// # Returns
/// Current picture number (PicNum)
pub fn calc_pic_num(frame_num: u32, max_frame_num: u32) -> i32 {
    // For frame pictures: PicNum = frame_num
    // For field pictures: more complex (not implemented yet)
    frame_num as i32
}

/// Calculate MaxPicNum from log2_max_frame_num
///
/// ISO/IEC 14496-10:2022 §8.2.4.1
pub fn calc_max_pic_num(log2_max_frame_num: u8) -> i32 {
    1 << log2_max_frame_num
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestPicture {
        id: u32,
    }

    #[test]
    fn test_rplr_command_from_idc() {
        let cmd = RplrCommand::from_idc(0, 5).unwrap();
        assert_eq!(
            cmd,
            RplrCommand::ShortTermSub {
                abs_diff_pic_num: 5
            }
        );

        let cmd = RplrCommand::from_idc(1, 3).unwrap();
        assert_eq!(
            cmd,
            RplrCommand::ShortTermAdd {
                abs_diff_pic_num: 3
            }
        );

        let cmd = RplrCommand::from_idc(2, 2).unwrap();
        assert_eq!(cmd, RplrCommand::LongTerm { long_term_pic_num: 2 });

        let cmd = RplrCommand::from_idc(3, 0).unwrap();
        assert_eq!(cmd, RplrCommand::End);
    }

    #[test]
    fn test_rplr_command_to_idc() {
        assert_eq!(
            RplrCommand::ShortTermSub {
                abs_diff_pic_num: 5
            }
            .to_idc(),
            0
        );
        assert_eq!(
            RplrCommand::ShortTermAdd {
                abs_diff_pic_num: 3
            }
            .to_idc(),
            1
        );
        assert_eq!(RplrCommand::LongTerm { long_term_pic_num: 2 }.to_idc(), 2);
        assert_eq!(RplrCommand::End.to_idc(), 3);
    }

    #[test]
    fn test_rplr_state_creation() {
        let state = RplrState::new();
        assert!(!state.list0_reordering);
        assert!(!state.list1_reordering);
        assert_eq!(state.list0_commands.len(), 0);
        assert_eq!(state.list1_commands.len(), 0);
    }

    #[test]
    fn test_rplr_state_add_commands() {
        let mut state = RplrState::new();

        state.add_list0_command(RplrCommand::ShortTermSub {
            abs_diff_pic_num: 1,
        });
        assert!(state.list0_reordering);
        assert_eq!(state.list0_commands.len(), 1);

        state.add_list1_command(RplrCommand::LongTerm {
            long_term_pic_num: 0,
        });
        assert!(state.list1_reordering);
        assert_eq!(state.list1_commands.len(), 1);
    }

    #[test]
    fn test_reorder_ref_pic_list_no_reordering() {
        let pic1 = TestPicture { id: 1 };
        let pic2 = TestPicture { id: 2 };

        let default_list = vec![
            RefPicListEntry {
                picture: &pic1,
                pic_num: 0,
                long_term_pic_num: None,
                is_long_term: false,
            },
            RefPicListEntry {
                picture: &pic2,
                pic_num: 1,
                long_term_pic_num: None,
                is_long_term: false,
            },
        ];

        let commands = vec![];
        let result = reorder_ref_pic_list(default_list, &commands, 2, 16).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].picture.id, 1);
        assert_eq!(result[1].picture.id, 2);
    }

    #[test]
    fn test_reorder_ref_pic_list_short_term_sub() {
        let pic1 = TestPicture { id: 1 };
        let pic2 = TestPicture { id: 2 };
        let pic3 = TestPicture { id: 3 };

        let default_list = vec![
            RefPicListEntry {
                picture: &pic1,
                pic_num: 2,
                long_term_pic_num: None,
                is_long_term: false,
            },
            RefPicListEntry {
                picture: &pic2,
                pic_num: 1,
                long_term_pic_num: None,
                is_long_term: false,
            },
            RefPicListEntry {
                picture: &pic3,
                pic_num: 0,
                long_term_pic_num: None,
                is_long_term: false,
            },
        ];

        // curr_pic_num = 4, abs_diff = 1, so pic_num = 4 - 1 - 1 = 2
        let commands = vec![
            RplrCommand::ShortTermSub {
                abs_diff_pic_num: 1,
            },
            RplrCommand::End,
        ];

        let result = reorder_ref_pic_list(default_list, &commands, 4, 16).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].pic_num, 2); // Moved to front
        assert_eq!(result[0].picture.id, 1);
    }

    #[test]
    fn test_reorder_ref_pic_list_long_term() {
        let pic1 = TestPicture { id: 1 };
        let pic2 = TestPicture { id: 2 };

        let default_list = vec![
            RefPicListEntry {
                picture: &pic1,
                pic_num: 0,
                long_term_pic_num: None,
                is_long_term: false,
            },
            RefPicListEntry {
                picture: &pic2,
                pic_num: 0,
                long_term_pic_num: Some(0),
                is_long_term: true,
            },
        ];

        let commands = vec![
            RplrCommand::LongTerm {
                long_term_pic_num: 0,
            },
            RplrCommand::End,
        ];

        let result = reorder_ref_pic_list(default_list, &commands, 2, 16).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result[0].is_long_term);
        assert_eq!(result[0].picture.id, 2);
    }

    #[test]
    fn test_calc_pic_num() {
        assert_eq!(calc_pic_num(0, 16), 0);
        assert_eq!(calc_pic_num(5, 16), 5);
        assert_eq!(calc_pic_num(15, 16), 15);
    }

    #[test]
    fn test_calc_max_pic_num() {
        assert_eq!(calc_max_pic_num(4), 16);  // 2^4 = 16
        assert_eq!(calc_max_pic_num(5), 32);  // 2^5 = 32
        assert_eq!(calc_max_pic_num(10), 1024); // 2^10 = 1024
    }
}
