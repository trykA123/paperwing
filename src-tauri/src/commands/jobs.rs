use crate::discover_job;
use crate::finder_service;
use crate::search_service;

domain! {
            discover_job::discover_start,
            discover_job::discover_cancel,
            discover_job::discover_cancel_all,
            finder_service::finder_start,
            search_service::search_start,
            search_service::search_cancel,
            search_service::search_cancel_all,
            search_service::search_capabilities,
}
