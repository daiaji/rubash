//! Bash-observable job identity and completion state.
//!
//! Windows process handles and waiting primitives stay in the executor/backend.

use std::collections::{BTreeMap, HashMap};

pub type JobId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Stopped,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub state: ProcessState,
    pub exit_status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobEntry {
    pub id: JobId,
    pub pids: Vec<u32>,
    pub processes: BTreeMap<u32, ProcessEntry>,
    pub command: String,
    pub state: ProcessState,
    pub exit_status: Option<i32>,
    pub background: bool,
    pub foreground: bool,
    pub notified: bool,
    pub coproc_endpoints: Vec<u32>,
    /// GNU jobs.c J_JOBCONTROL: whether the job was started while job
    /// control (`set -m`) was active. fg/bg must refuse jobs without it
    /// (fg_bg.def:159 "job %d started without job control").
    pub job_control: bool,
}

#[derive(Debug, Default, Clone)]
pub struct JobTable {
    pub jobs: BTreeMap<JobId, JobEntry>,
    pub pid_to_job: HashMap<u32, JobId>,
    pub completed_statuses: HashMap<u32, i32>,
    current_job: Option<JobId>,
    previous_job: Option<JobId>,
}

impl JobTable {
    pub fn register_process(
        &mut self,
        pid: u32,
        command: impl Into<String>,
        background: bool,
    ) -> JobId {
        self.register_pipeline(vec![pid], command, background)
    }

    pub fn register_pipeline(
        &mut self,
        pids: Vec<u32>,
        command: impl Into<String>,
        background: bool,
    ) -> JobId {
        // GNU jobs.c:586-614 alloc_job_entry: a non-interactive shell scans
        // forward from js.j_lastj and takes the first free slot — the job
        // number is the slot index, which keeps holes open for jobs that
        // were reaped earlier and is only lowered again when the tail of
        // the table is freed (delete_job/cleanup_dead_jobs recompute
        // j_lastj, jobs.c:1270-1292). The next id is therefore
        // highest-occupied-id + 1, NOT a monotonic counter and NOT the
        // lowest free slot: with {1,3} live, the next job is %4; once %3
        // is removed the next is %2.
        let id = self
            .jobs
            .keys()
            .next_back()
            .map_or(1, |highest| highest.saturating_add(1));
        let processes = pids
            .iter()
            .copied()
            .map(|pid| {
                (
                    pid,
                    ProcessEntry {
                        pid,
                        state: ProcessState::Running,
                        exit_status: None,
                    },
                )
            })
            .collect();
        let entry = JobEntry {
            id,
            pids: pids.clone(),
            processes,
            command: command.into(),
            state: ProcessState::Running,
            exit_status: None,
            background,
            foreground: !background,
            notified: false,
            coproc_endpoints: Vec::new(),
            job_control: false,
        };
        for pid in pids {
            self.pid_to_job.insert(pid, id);
        }
        self.jobs.insert(id, entry);
        self.set_current_job(id);
        id
    }

    pub fn resolve_jobspec(&self, spec: &str) -> Option<JobId> {
        let body = spec.strip_prefix('%').unwrap_or(spec);
        if body.is_empty() || matches!(body, "%" | "+") {
            return self
                .current_job
                .or_else(|| self.jobs.keys().next_back().copied());
        }
        if body == "-" {
            return self.previous_job;
        }
        if let Some(id) = body.strip_prefix('?') {
            return self
                .jobs
                .iter()
                .rev()
                .find(|(_, job)| job.command.contains(id))
                .map(|(id, _)| *id);
        }
        if let Ok(id) = body.parse::<JobId>() {
            return self.jobs.contains_key(&id).then_some(id);
        }
        self.jobs
            .iter()
            .rev()
            .find(|(_, job)| job.command.starts_with(body))
            .map(|(id, _)| *id)
    }

    pub fn current_job(&self) -> Option<JobId> {
        self.current_job
    }

    pub fn previous_job(&self) -> Option<JobId> {
        self.previous_job
    }

    /// GNU jobs.c:3668 most_recent_job_in_state: newest job with an id
    /// below `below` in the requested state.
    fn most_recent_in_state(&self, below: JobId, state: ProcessState) -> Option<JobId> {
        self.jobs
            .range(..below)
            .rev()
            .find(|(_, job)| job.state == state)
            .map(|(id, _)| *id)
    }

    /// GNU jobs.c:3705-3757 set_current_job: JOB becomes current; previous
    /// is (1) the old current if still a stopped job, (2) the newest
    /// stopped job older than current when current is stopped, (3) the
    /// newest running job older than current (or newest overall when
    /// current is stopped), (4) current itself when it is the only job.
    pub fn set_current_job(&mut self, id: JobId) {
        if self.current_job != Some(id) {
            self.previous_job = self.current_job;
            self.current_job = Some(id);
        }
        let Some(current) = self.current_job else {
            return;
        };
        if self.previous_job != Some(current)
            && self
                .previous_job
                .and_then(|prev| self.jobs.get(&prev))
                .is_some_and(|job| job.state == ProcessState::Stopped)
        {
            return;
        }
        let current_stopped = self
            .jobs
            .get(&current)
            .is_some_and(|job| job.state == ProcessState::Stopped);
        if current_stopped {
            if let Some(candidate) = self.most_recent_in_state(current, ProcessState::Stopped) {
                self.previous_job = Some(candidate);
                return;
            }
        }
        let running_candidate = if !current_stopped {
            self.most_recent_in_state(current, ProcessState::Running)
        } else {
            self.jobs
                .iter()
                .rev()
                .find(|(_, job)| job.state == ProcessState::Running)
                .map(|(id, _)| *id)
        };
        match running_candidate {
            Some(candidate) => self.previous_job = Some(candidate),
            None => self.previous_job = Some(current),
        }
    }

    /// GNU jobs.c:3759-3797 reset_current: recompute current/previous after
    /// jobs die or are deleted — current stays if it is a stopped job, else
    /// the stopped previous, else the newest stopped, else the newest
    /// running job; NO_JOB when the table is empty.
    pub fn reset_current(&mut self) {
        let current_stopped = self
            .current_job
            .and_then(|id| self.jobs.get(&id))
            .is_some_and(|job| job.state == ProcessState::Stopped);
        let candidate = if current_stopped {
            self.current_job
        } else {
            let previous_stopped = self
                .previous_job
                .and_then(|id| self.jobs.get(&id))
                .is_some_and(|job| job.state == ProcessState::Stopped);
            if previous_stopped {
                self.previous_job
            } else {
                self.most_recent_in_state(JobId::MAX, ProcessState::Stopped)
                    .or_else(|| self.most_recent_in_state(JobId::MAX, ProcessState::Running))
            }
        };
        match candidate {
            Some(id) => self.set_current_job(id),
            None => {
                self.current_job = None;
                self.previous_job = None;
            }
        }
    }

    pub fn mark_completed(&mut self, pid: u32, status: i32) {
        self.completed_statuses.insert(pid, status);
        let Some(job_id) = self.pid_to_job.get(&pid).copied() else {
            return;
        };
        let Some(job) = self.jobs.get_mut(&job_id) else {
            return;
        };
        if let Some(process) = job.processes.get_mut(&pid) {
            process.state = ProcessState::Completed;
            process.exit_status = Some(status);
        }
        // jobs.c:2219 contract — waitchld clears J_NOTIFIED on a state
        // change so the next `jobs -n` reports this job again.
        job.notified = false;
        self.recompute_job(job_id);
    }

    /// GNU jobs.c J_JOBCONTROL flag, recorded at spawn: fg/bg refuse jobs
    /// started without job control (fg_bg.def:159).
    pub fn set_job_control(&mut self, pid: u32, job_control: bool) {
        if let Some(job_id) = self.pid_to_job.get(&pid).copied() {
            if let Some(job) = self.jobs.get_mut(&job_id) {
                job.job_control = job_control;
            }
        }
    }

    pub fn job_control_for_pid(&self, pid: u32) -> bool {
        self.pid_to_job
            .get(&pid)
            .and_then(|job_id| self.jobs.get(job_id))
            .is_some_and(|job| job.job_control)
    }

    pub fn job_id_for_pid(&self, pid: u32) -> Option<JobId> {
        self.pid_to_job.get(&pid).copied()
    }

    pub fn mark_stopped(&mut self, pid: u32) {
        let Some(job_id) = self.pid_to_job.get(&pid).copied() else {
            return;
        };
        if let Some(job) = self.jobs.get_mut(&job_id) {
            if let Some(process) = job.processes.get_mut(&pid) {
                process.state = ProcessState::Stopped;
            }
            job.notified = false;
        }
        self.recompute_job(job_id);
        // GNU jobs.c waitchld: a job that has just stopped becomes the
        // current job.
        self.set_current_job(job_id);
    }

    pub fn mark_running(&mut self, pid: u32) {
        let Some(job_id) = self.pid_to_job.get(&pid).copied() else {
            return;
        };
        if let Some(job) = self.jobs.get_mut(&job_id) {
            if let Some(process) = job.processes.get_mut(&pid) {
                process.state = ProcessState::Running;
            }
        }
        // jobs.c start_job/set_job_running: a Running transition does not
        // pick the job as current. Callers decide: fg -> set_current_job,
        // bg -> reset_current.
        self.recompute_job(job_id);
    }

    fn recompute_job(&mut self, job_id: JobId) {
        let Some(job) = self.jobs.get_mut(&job_id) else {
            return;
        };
        let any_running = job
            .processes
            .values()
            .any(|p| p.state == ProcessState::Running);
        let any_stopped = job
            .processes
            .values()
            .any(|p| p.state == ProcessState::Stopped);
        job.state = if any_running {
            ProcessState::Running
        } else if any_stopped {
            ProcessState::Stopped
        } else {
            ProcessState::Completed
        };
        job.exit_status = job
            .pids
            .last()
            .and_then(|pid| job.processes.get(pid))
            .and_then(|process| process.exit_status);
    }

    /// jobs.c:1303 cleanup_dead_jobs — delete dead jobs the user was
    /// already notified about (POSIX: a terminated job leaves the list once
    /// `jobs` reports it). Completed statuses stay in `completed_statuses`
    /// (the bgpids equivalent) so a later operand-addressed `wait $pid`
    /// still reports the exit status.
    pub fn cleanup_dead_jobs(&mut self) {
        let dead_notified: Vec<JobId> = self
            .jobs
            .iter()
            .filter(|(_, job)| job.state == ProcessState::Completed && job.notified)
            .map(|(job_id, _)| *job_id)
            .collect();
        let mut removed_current_or_previous = false;
        for job_id in dead_notified {
            removed_current_or_previous |=
                Some(job_id) == self.current_job || Some(job_id) == self.previous_job;
            if let Some(job) = self.jobs.remove(&job_id) {
                for pid in &job.pids {
                    self.pid_to_job.remove(pid);
                }
            }
        }
        // GNU delete_job (jobs.c:1535-1543): current/previous are reset only
        // when the deleted job held one of the markers; deleting any other
        // job leaves them alone.
        if removed_current_or_previous {
            self.reset_current();
        }
    }

    /// jobs.c:3652 reap_dead_jobs — non-interactive shells (and shells
    /// without job control) remove dead jobs without printing:
    /// mark_dead_jobs_as_notified + cleanup_dead_jobs. GNU calls this via
    /// the REAP() macro after every loop body (execute_cmd.c:2979,
    /// for/select/while/until/arith-for) and from compact_jobs_list.
    ///
    /// jobs.c:5179-5230 mark_dead_jobs_as_notified keeps CHILD_MAX
    /// (DEFAULT_CHILD_MAX 4096, jobs.c:92) dead processes unnotified so
    /// `wait` can still report their statuses, and never marks the job
    /// holding last_asynchronous_pid ($!) — so REAP is a no-op for any
    /// realistic script unless a listing already reported the job.
    pub fn reap_dead_jobs(&mut self, last_asynchronous_pid: Option<u32>) {
        const CHILD_MAX: usize = 4096;
        let dead_processes: usize = self
            .jobs
            .values()
            .filter(|job| job.state == ProcessState::Completed)
            .map(|job| job.pids.len())
            .sum();
        if dead_processes > CHILD_MAX {
            let mut keep = dead_processes;
            for job in self.jobs.values_mut() {
                if job.state != ProcessState::Completed {
                    continue;
                }
                if keep <= CHILD_MAX {
                    break;
                }
                if job.pids.last().copied() == last_asynchronous_pid {
                    continue;
                }
                job.notified = true;
                keep = keep.saturating_sub(job.pids.len());
            }
        }
        self.cleanup_dead_jobs();
    }

    pub fn reap_finished<I>(&mut self, statuses: I)
    where
        I: IntoIterator<Item = (u32, i32)>,
    {
        for (pid, status) in statuses {
            self.mark_completed(pid, status);
        }
    }

    pub fn wait_pid(&mut self, pid: u32) -> Option<i32> {
        self.completed_statuses.remove(&pid).or_else(|| {
            let job_id = self.pid_to_job.get(&pid).copied()?;
            self.jobs.get(&job_id)?.exit_status
        })
    }

    pub fn wait_any(&mut self) -> Option<(u32, i32)> {
        let pid = self.completed_statuses.keys().next().copied()?;
        let status = self.completed_statuses.remove(&pid)?;
        Some((pid, status))
    }

    pub fn wait_all(&mut self) -> Vec<(u32, i32)> {
        let mut result: Vec<_> = self.completed_statuses.drain().collect();
        result.sort_by_key(|(pid, _)| *pid);
        result
    }

    pub fn remove_job(&mut self, job_id: JobId) -> bool {
        let Some(job) = self.jobs.remove(&job_id) else {
            return false;
        };
        for pid in &job.pids {
            self.pid_to_job.remove(pid);
            self.completed_statuses.remove(pid);
        }
        // jobs.c:1541-1543 delete_job: reset_current only when the removed
        // job held the current or previous marker.
        if Some(job_id) == self.current_job || Some(job_id) == self.previous_job {
            self.reset_current();
        }
        true
    }

    pub fn remove_job_by_pid(&mut self, pid: u32) -> bool {
        self.pid_to_job
            .get(&pid)
            .copied()
            .is_some_and(|job_id| self.remove_job(job_id))
    }

    pub fn remove_job_by_pid_preserve_status(&mut self, pid: u32) -> bool {
        let Some(job_id) = self.pid_to_job.get(&pid).copied() else {
            return false;
        };
        let Some(job) = self.jobs.remove(&job_id) else {
            return false;
        };
        for candidate in &job.pids {
            self.pid_to_job.remove(candidate);
        }
        if Some(job_id) == self.current_job || Some(job_id) == self.previous_job {
            self.reset_current();
        }
        true
    }

    pub fn clear_jobs(&mut self) {
        self.jobs.clear();
        self.pid_to_job.clear();
        self.completed_statuses.clear();
        self.current_job = None;
        self.previous_job = None;
    }

    pub fn attach_coproc_endpoint(&mut self, job_id: JobId, fd: u32) {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.coproc_endpoints.push(fd);
        }
    }

    pub fn take_coproc_endpoint(&mut self, job_id: JobId, fd: u32) -> bool {
        self.jobs.get_mut(&job_id).map_or(false, |job| {
            let before = job.coproc_endpoints.len();
            job.coproc_endpoints.retain(|candidate| *candidate != fd);
            before != job.coproc_endpoints.len()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_can_be_waited_explicitly_after_reaping() {
        let mut table = JobTable::default();
        table.register_process(42, "sleep", true);
        table.reap_finished([(42, 7)]);
        assert_eq!(table.wait_pid(42), Some(7));
    }

    #[test]
    fn jobspecs_resolve_in_job_order() {
        let mut table = JobTable::default();
        let first = table.register_process(1, "one", true);
        let second = table.register_process(2, "two", true);
        assert_eq!(table.resolve_jobspec("%+"), Some(second));
        assert_eq!(table.resolve_jobspec("%-"), Some(first));
        assert_eq!(table.resolve_jobspec("%1"), Some(first));
    }

    #[test]
    fn pipeline_state_waits_for_all_processes_and_uses_last_status() {
        let mut table = JobTable::default();
        let job = table.register_pipeline(vec![10, 11], "producer | consumer", true);
        assert_eq!(table.jobs[&job].state, ProcessState::Running);
        table.mark_completed(10, 3);
        assert_eq!(table.jobs[&job].state, ProcessState::Running);
        table.mark_completed(11, 7);
        assert_eq!(table.jobs[&job].state, ProcessState::Completed);
        assert_eq!(table.jobs[&job].exit_status, Some(7));
    }

    #[test]
    fn stopped_and_continued_processes_update_job_state() {
        let mut table = JobTable::default();
        let job = table.register_process(20, "sleep", true);
        table.mark_stopped(20);
        assert_eq!(table.jobs[&job].state, ProcessState::Stopped);
        table.mark_running(20);
        assert_eq!(table.jobs[&job].state, ProcessState::Running);
    }

    #[test]
    fn removing_a_job_clears_pid_and_current_previous_indexes() {
        let mut table = JobTable::default();
        let first = table.register_process(30, "first", true);
        table.register_process(31, "second", true);
        assert!(table.remove_job_by_pid(31));
        assert!(!table.pid_to_job.contains_key(&31));
        assert_eq!(table.resolve_jobspec("%+"), Some(first));
        assert_eq!(table.resolve_jobspec("%1"), Some(first));
        assert!(table.remove_job(first));
        assert!(table.jobs.is_empty());
    }

    #[test]
    fn wait_minus_removes_job_but_retains_explicit_pid_status() {
        let mut table = JobTable::default();
        let job = table.register_process(40, "false", true);
        table.mark_completed(40, 1);
        assert!(table.remove_job_by_pid_preserve_status(40));
        assert!(!table.jobs.contains_key(&job));
        assert_eq!(table.completed_statuses.get(&40), Some(&1));
        assert_eq!(table.pid_to_job.get(&40), None);
        assert_eq!(table.wait_pid(40), Some(1));
    }
}
