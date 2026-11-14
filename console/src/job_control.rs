//! Bounded independent transfers with task-local cancellation and resource ownership.
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
#[derive(Default)]
pub struct Jobs(Mutex<BTreeMap<String, Entry>>);
struct Entry {
    resources: Vec<String>,
    cancellable: bool,
    cancel: Arc<AtomicBool>,
}
pub struct Lease {
    jobs: Arc<Jobs>,
    id: String,
    pub cancel: Arc<AtomicBool>,
}
impl Jobs {
    pub fn reserve(
        self: &Arc<Self>,
        id: &str,
        resources: Vec<String>,
        cancellable: bool,
    ) -> Result<Lease, String> {
        let mut jobs = self.0.lock().unwrap();
        if jobs
            .values()
            .any(|e| e.resources.iter().any(|r| resources.contains(r)))
        {
            return Err("该资源已有任务运行，请等待完成".into());
        }
        if jobs.len() >= 3 {
            return Err("已有 3 个下载任务运行，请等待其中一个完成".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        jobs.insert(
            id.into(),
            Entry {
                resources,
                cancellable,
                cancel: cancel.clone(),
            },
        );
        Ok(Lease {
            jobs: self.clone(),
            id: id.into(),
            cancel,
        })
    }
    pub fn cancel(&self, id: Option<&str>) -> Result<String, String> {
        let jobs = self.0.lock().unwrap();
        let id = match id {
            Some(id) => id,
            None if jobs.len() == 1 => jobs.keys().next().unwrap(),
            None => return Err("请指定要中断的任务".into()),
        };
        let e = jobs.get(id).ok_or("该任务已结束或不存在")?;
        if !e.cancellable {
            return Err("宿主机安装或修复中，不能强制取消包管理器；请等待任务完成".into());
        }
        e.cancel.store(true, Ordering::SeqCst);
        Ok(id.into())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.jobs.0.lock().unwrap().remove(&self.id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_resources_and_cancellation_are_independent() {
        let jobs = Arc::new(Jobs::default());
        let a = jobs.reserve("a", vec!["image:a".into()], true).unwrap();
        let b = jobs.reserve("b", vec!["image:b".into()], true).unwrap();
        assert!(jobs.reserve("same", vec!["image:a".into()], true).is_err());
        assert!(jobs.cancel(None).is_err());
        jobs.cancel(Some("a")).unwrap();
        assert!(a.cancel.load(Ordering::SeqCst));
        assert!(!b.cancel.load(Ordering::SeqCst));
        let c = jobs.reserve("c", vec!["weights:c".into()], true).unwrap();
        assert!(jobs.reserve("d", vec!["image:d".into()], true).is_err());
        drop(a);
        assert!(jobs.cancel(Some("a")).is_err());
        let d = jobs.reserve("d", vec!["image:a".into()], false).unwrap();
        assert!(jobs.cancel(Some("d")).is_err());
        drop((b, c, d));
        assert!(jobs.reserve("retry", vec!["image:a".into()], true).is_ok());
    }
}
