#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchStatus {
    Dispatched,
    OnSite,
    Completed,
}

#[derive(Debug, Eq, PartialEq)]
pub enum FollowUp {
    KeepSessionOpen,
    ArchivePhotos { object_prefix: String },
}

pub fn decide_follow_up(work_order_id: &str, status: DispatchStatus) -> FollowUp {
    match status {
        DispatchStatus::Completed => FollowUp::ArchivePhotos {
            object_prefix: format!("work-orders/{work_order_id}/photos"),
        },
        DispatchStatus::Dispatched | DispatchStatus::OnSite => FollowUp::KeepSessionOpen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_dispatch_hands_photos_to_the_private_archive() {
        let result = decide_follow_up("wo-1842", DispatchStatus::Completed);

        assert_eq!(
            result,
            FollowUp::ArchivePhotos {
                object_prefix: "work-orders/wo-1842/photos".to_owned()
            }
        );
        assert_eq!(
            decide_follow_up("wo-1842", DispatchStatus::OnSite),
            FollowUp::KeepSessionOpen
        );
    }
}
