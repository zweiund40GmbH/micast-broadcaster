

use std::sync::RwLock;
use std::sync::Arc;
use log::{debug};

#[derive(Debug)]
pub struct Queue {
    inner: RwLock<Vec<Arc<super::Item>>>,
}

impl Queue {

    pub fn new() -> Queue {
        Queue { inner: RwLock::new(Vec::new())}
    }

    pub(crate) fn push(&self, value: super::Item) {
        let mut inner = self.inner.write().unwrap();
        inner.push(Arc::new(value));
        drop(inner);
    }


    fn next_check(&self, retry_count: Option<usize>) -> Option<Arc<super::Item>> {
        let items = self.inner.write().unwrap();

        let mut index = 0;
        let mut found = false;

        while let Some(item) = items.get(index) {            
            if item.state() == super::ItemState::Prepared {
                found = true;
                debug!("prepared item found at {} uri {}", index, item.uri);
                break;
            }
            index += 1;
        }

        
        let item = if let Some(i) = items.get(index) {
            Some(i.clone())
        } else {
            None
        };

        let has_unknown = items.iter().find(|&i| i.state() == super::ItemState::Unknown).is_some();

        drop(items);

        if found == false {
            if has_unknown == true  {
                debug!("no prepared items found, put we have some unknowns... we wait 500millis");
                if retry_count.unwrap() > 0 {
                    let r = retry_count.unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    return self.next_check(Some(r - 1))
                }
            }
            return None;
        }
        
        item
        
    }



    pub(crate) fn next(&self) -> Option<Arc<super::Item>> {
        self.next_check(Some(5))
    }

    pub(crate) fn current(&self) -> Option<Arc<super::Item>> {
        let inner = self.inner.write().unwrap();
        for item in inner.iter() {
            debug!("item in list: {} - {:?}", item.uri, item.state());
        }
        let item = inner.iter().find(|item| item.state() == super::ItemState::Activate);

        if let Some(v) = item {
            if v.state() == super::ItemState::Activate {
                let item = v.clone();
                return Some(item)
            }
        }
        
        None
    }

    pub fn clean(&self) {
        debug!("clean queue");
        let mut inner = self.inner.write().unwrap();
        inner.retain(|item| item.state() != super::ItemState::Removed );
        for item in inner.iter() {
            debug!("item {}, {:?}", item.uri, item.state());
        }
        drop(inner);
    }

    /// returns the size of the queue
    pub fn length(&self) -> usize {
        let inner = self.inner.read().unwrap();
        let length = inner.len();
        drop(inner);

        length
    }
}