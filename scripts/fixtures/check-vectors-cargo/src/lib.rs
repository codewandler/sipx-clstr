#![allow(dead_code)]

#[cfg(test)]
mod tests {
    fn pb_v_991_plain_helper_is_not_executable() {}

    #[test]
    #[ignore = "CF-20 fixture: listing it must not make it executable evidence"]
    fn pb_v_992_ignored_test_is_not_executable_evidence() {}

    #[cfg(any())]
    #[test]
    fn pb_v_993_inactive_cfg_test_is_not_discoverable() {}

    #[test]
    fn pb_v_994_executable_test_is_evidence() {}

    // covers: PB-V-995
    #[test]
    fn a_differently_named_executable_test_is_evidence() {}

    // covers: PB-V-996
    fn a_covers_comment_cannot_bind_to_a_plain_helper() {}

    mod duplicate_first {
        #[test]
        fn pb_v_998_duplicate_source_name_is_ambiguous() {}
    }

    mod duplicate_second {
        #[test]
        fn pb_v_998_duplicate_source_name_is_ambiguous() {}
    }

    mod boundary_first {
        // covers: PB-V-999
    }

    mod boundary_second {
        #[test]
        fn a_later_test_in_another_module_is_not_the_comments_item() {}
    }

    const STRING_CLAIM: &str = "// covers: PB-V-1000";

    #[test]
    fn a_string_literal_is_not_a_coverage_declaration() {}

    const TRAILING_CLAIM: &str = "not a declaration"; // covers: PB-V-1001

    #[test]
    fn a_trailing_comment_is_not_a_coverage_declaration() {}

    // covers: PB-V-1002
    #[doc = "["]
    const NOT_THE_TEST: &str = "]";

    #[test]
    fn brackets_in_attribute_strings_cannot_swallow_an_intervening_item() {}

    // covers: PB-V-1003
    #[doc = "["]
    #[test]
    fn brackets_in_attribute_strings_still_allow_a_real_test_item() {}

    // covers: PB-V-997
}
