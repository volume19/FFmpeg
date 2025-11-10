//! Reference Picture List Construction
//!
//! Constructs reference picture lists (List 0 and List 1) for P and B slices.
//! Implements ISO/IEC 14496-10:2022 §8.2.4 processes.

use super::dpb::{Dpb, RefPicture};
use super::rplr::RplrCommand;
use av_core::Result;

/// Reference picture list type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefPicListType {
    /// List 0: For P and B slices
    List0,
    /// List 1: For B slices only
    List1,
}

/// Reference picture list entry
#[derive(Debug, Clone)]
pub struct RefPicListEntry<'a> {
    /// Reference to the picture
    pub picture: &'a RefPicture,
    /// Picture number (for short-term references)
    pub pic_num: Option<i32>,
    /// Long-term picture number (for long-term references)
    pub long_term_pic_num: Option<i32>,
}

/// Reference picture lists for a slice
pub struct RefPicLists<'a> {
    /// List 0 (for P and B slices)
    pub list0: Vec<RefPicListEntry<'a>>,
    /// List 1 (for B slices only)
    pub list1: Vec<RefPicListEntry<'a>>,
}

impl<'a> RefPicLists<'a> {
    /// Create empty reference picture lists
    pub fn new() -> Self {
        Self {
            list0: Vec::new(),
            list1: Vec::new(),
        }
    }

    /// Get the specified list
    pub fn get(&self, list_type: RefPicListType) -> &[RefPicListEntry<'a>] {
        match list_type {
            RefPicListType::List0 => &self.list0,
            RefPicListType::List1 => &self.list1,
        }
    }

    /// Get mutable access to the specified list
    pub fn get_mut(&mut self, list_type: RefPicListType) -> &mut Vec<RefPicListEntry<'a>> {
        match list_type {
            RefPicListType::List0 => &mut self.list0,
            RefPicListType::List1 => &mut self.list1,
        }
    }
}

impl<'a> Default for RefPicLists<'a> {
    fn default() -> Self {
        Self::new()
    }
}

/// Construct reference picture list 0 for P slice
///
/// # Arguments
/// * `dpb` - Decoded picture buffer
/// * `curr_pic_num` - Current picture number
/// * `max_pic_num` - Maximum picture number (2^log2_max_frame_num)
///
/// # Spec Reference
/// ISO/IEC 14496-10:2022 §8.2.4.2.1 (Initialization process for P slices)
pub fn construct_p_slice_list0<'a>(
    dpb: &'a Dpb,
    curr_pic_num: i32,
    _max_pic_num: i32,
) -> Vec<RefPicListEntry<'a>> {
    let mut list = Vec::new();

    // Get short-term references sorted by descending PicNum
    let short_term = dpb.get_short_term_list(curr_pic_num, true);

    for pic in short_term {
        list.push(RefPicListEntry {
            picture: pic,
            pic_num: Some(pic.frame_num as i32), // Simplified: frame_num = pic_num for this context
            long_term_pic_num: None,
        });
    }

    // Append long-term references sorted by ascending LongTermPicNum
    let long_term = dpb.get_long_term_list();

    for pic in long_term {
        list.push(RefPicListEntry {
            picture: pic,
            pic_num: None,
            long_term_pic_num: Some(pic.long_term_frame_idx.unwrap_or(0) as i32),
        });
    }

    list
}

/// Construct reference picture lists for B slice
///
/// # Arguments
/// * `dpb` - Decoded picture buffer
/// * `curr_poc` - Current picture order count
/// * `max_pic_num` - Maximum picture number
///
/// # Spec Reference
/// ISO/IEC 14496-10:2022 §8.2.4.2.3 (Initialization process for B slices)
pub fn construct_b_slice_lists<'a>(
    dpb: &'a Dpb,
    curr_poc: i32,
    _max_pic_num: i32,
) -> RefPicLists<'a> {
    let mut lists = RefPicLists::new();

    // Get all short-term references
    let all_refs: Vec<&RefPicture> = dpb.get_short_term_list(0, false);

    // Split into past (POC < curr_poc) and future (POC > curr_poc)
    let mut past_refs: Vec<(&RefPicture, i32)> = all_refs
        .iter()
        .filter_map(|pic| {
            if pic.pic_order_cnt < curr_poc {
                Some((*pic, pic.pic_order_cnt))
            } else {
                None
            }
        })
        .collect();

    let mut future_refs: Vec<(&RefPicture, i32)> = all_refs
        .iter()
        .filter_map(|pic| {
            if pic.pic_order_cnt >= curr_poc {
                Some((*pic, pic.pic_order_cnt))
            } else {
                None
            }
        })
        .collect();

    // Sort past refs by descending POC (most recent first)
    past_refs.sort_by(|a, b| b.1.cmp(&a.1));

    // Sort future refs by ascending POC (nearest first)
    future_refs.sort_by(|a, b| a.1.cmp(&b.1));

    // List 0: past refs (desc POC) + future refs (asc POC) + long-term
    for (pic, _) in &past_refs {
        lists.list0.push(RefPicListEntry {
            picture: pic,
            pic_num: Some(pic.frame_num as i32),
            long_term_pic_num: None,
        });
    }

    for (pic, _) in &future_refs {
        lists.list0.push(RefPicListEntry {
            picture: pic,
            pic_num: Some(pic.frame_num as i32),
            long_term_pic_num: None,
        });
    }

    // Add long-term references to list 0
    for pic in dpb.get_long_term_list() {
        lists.list0.push(RefPicListEntry {
            picture: pic,
            pic_num: None,
            long_term_pic_num: Some(pic.long_term_frame_idx.unwrap_or(0) as i32),
        });
    }

    // List 1: future refs (asc POC) + past refs (desc POC) + long-term
    for (pic, _) in &future_refs {
        lists.list1.push(RefPicListEntry {
            picture: pic,
            pic_num: Some(pic.frame_num as i32),
            long_term_pic_num: None,
        });
    }

    for (pic, _) in &past_refs {
        lists.list1.push(RefPicListEntry {
            picture: pic,
            pic_num: Some(pic.frame_num as i32),
            long_term_pic_num: None,
        });
    }

    // Add long-term references to list 1
    for pic in dpb.get_long_term_list() {
        lists.list1.push(RefPicListEntry {
            picture: pic,
            pic_num: None,
            long_term_pic_num: Some(pic.long_term_frame_idx.unwrap_or(0) as i32),
        });
    }

    // If list 1 has only one entry and is same as list 0[0], swap first two entries in list 1
    if lists.list1.len() == 1 && !lists.list0.is_empty() {
        if let (Some(entry0), Some(entry1)) = (lists.list0.first(), lists.list1.first()) {
            if std::ptr::eq(entry0.picture, entry1.picture) && lists.list1.len() > 1 {
                lists.list1.swap(0, 1);
            }
        }
    }

    lists
}

/// Apply reference picture list reordering commands
///
/// # Arguments
/// * `default_list` - Default reference picture list
/// * `commands` - RPLR commands for reordering
/// * `_curr_pic_num` - Current picture number
/// * `_max_pic_num` - Maximum picture number
///
/// # Returns
/// Reordered reference picture list
///
/// # Note
/// Currently returns the default list without reordering.
/// Full RPLR implementation can be added later by adapting rplr::reorder_ref_pic_list.
pub fn apply_rplr<'a>(
    default_list: Vec<RefPicListEntry<'a>>,
    commands: &[RplrCommand],
    _curr_pic_num: i32,
    _max_pic_num: i32,
) -> Result<Vec<RefPicListEntry<'a>>> {
    // TODO: Implement full RPLR logic when needed
    // For now, return default list if no commands or just ignore commands
    let _ = commands; // Suppress unused warning
    Ok(default_list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ref_pic_lists_new() {
        let lists: RefPicLists = RefPicLists::new();
        assert!(lists.list0.is_empty());
        assert!(lists.list1.is_empty());
    }

    #[test]
    fn test_ref_pic_lists_get() {
        let lists: RefPicLists = RefPicLists::new();
        assert_eq!(lists.get(RefPicListType::List0).len(), 0);
        assert_eq!(lists.get(RefPicListType::List1).len(), 0);
    }

    #[test]
    fn test_ref_pic_list_type() {
        assert_eq!(RefPicListType::List0, RefPicListType::List0);
        assert_ne!(RefPicListType::List0, RefPicListType::List1);
    }

    #[test]
    fn test_construct_p_slice_empty_dpb() {
        let dpb = Dpb::new(4);
        let list = construct_p_slice_list0(&dpb, 10, 256);
        assert!(list.is_empty());
    }

    #[test]
    fn test_construct_b_slice_empty_dpb() {
        let dpb = Dpb::new(4);
        let lists = construct_b_slice_lists(&dpb, 10, 256);
        assert!(lists.list0.is_empty());
        assert!(lists.list1.is_empty());
    }
}
