use super::*;
impl ConnectManager {
    #[cfg(feature = "connect-lab")]
    pub fn prepare_ready_game_launch(
        &self,
        owner: &str,
        request: ReadyGameLaunchRequest,
        backend: std::sync::Arc<dyn ReadyGameBackend>,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        self.live.prepare_ready_game(owner, request, backend)
    }
    pub fn ready_game_launch_snapshot(
        &self,
        owner: &str,
        operation: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.ready_game_snapshot(owner, operation)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation);
            Err(ReadyGameLaunchError::Disabled)
        }
    }
    pub fn confirm_ready_game_launch(
        &self,
        owner: &str,
        operation: &str,
        plan: &str,
        digest: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.confirm_ready_game(owner, operation, plan, digest)
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation, plan, digest);
            Err(ReadyGameLaunchError::Disabled)
        }
    }
    pub async fn cancel_ready_game_launch(
        &self,
        owner: &str,
        operation: &str,
        plan: &str,
    ) -> Result<ReadyGameLaunchSnapshot, ReadyGameLaunchError> {
        #[cfg(feature = "connect-lab")]
        {
            self.live.cancel_ready_game(owner, operation, plan).await
        }
        #[cfg(not(feature = "connect-lab"))]
        {
            let _ = (owner, operation, plan);
            Err(ReadyGameLaunchError::Disabled)
        }
    }
}

#[cfg(all(test, not(feature = "connect-lab")))]
mod tests {
    use super::*;
    #[test]
    fn ready_game_default_caller_is_disabled_without_resources() {
        let manager = ConnectManager::default();
        assert!(manager.capabilities().ready_game_launch.is_none());
        assert_eq!(
            manager
                .ready_game_launch_snapshot("main", "operation")
                .unwrap_err(),
            ReadyGameLaunchError::Disabled
        );
        assert_eq!(
            manager
                .confirm_ready_game_launch("main", "operation", "plan", "digest")
                .unwrap_err(),
            ReadyGameLaunchError::Disabled
        );
        let mut cancelled = Box::pin(manager.cancel_ready_game_launch("main", "operation", "plan"));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(matches!(
            std::future::Future::poll(cancelled.as_mut(), &mut context),
            std::task::Poll::Ready(Err(ReadyGameLaunchError::Disabled))
        ));
    }
}
