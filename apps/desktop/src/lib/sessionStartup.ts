export type SessionStartupState<Profile> =
  | { phase: "loading"; profile?: undefined; error?: undefined }
  | { phase: "signed-out"; profile?: undefined; error?: undefined }
  | { phase: "ready"; profile: Profile; error?: undefined }
  | { phase: "error"; profile?: undefined; error: string };

type Options<User, Profile> = {
  observe: (next: (user: User | null) => void, error: () => void) => () => void;
  currentUser: () => User | null;
  loadProfile: (user: User) => Promise<Profile>;
  onState: (state: SessionStartupState<Profile>) => void;
  timeoutMs?: number;
};

/** Late appearance/migration work from another account must not replace this profile. */
export function guardSessionProfile<Profile extends { uid: string }>(
  current: Profile | undefined,
  incoming: Profile | undefined,
  authenticatedUid: string | undefined,
): Profile | undefined {
  return incoming && incoming.uid !== authenticatedUid ? current : incoming;
}

/** Unknown auth state is never treated as signed out, nor as an authenticated profile. */
export function startSessionStartup<User extends { uid: string }, Profile extends { uid: string }>(
  options: Options<User, Profile>,
) {
  let disposed = false;
  let generation = 0;
  let authenticating = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let unsubscribe: (() => void) | undefined;
  const failure = "Não foi possível carregar sua sessão. Verifique a conexão e tente novamente.";

  function invalidate() {
    clearTimeout(timer);
    return ++generation;
  }

  function fail(attempt: number) {
    if (disposed || attempt !== generation) return;
    invalidate();
    options.onState({ phase: "error", error: failure });
  }

  function watch(attempt: number) {
    timer = setTimeout(() => fail(attempt), options.timeoutMs ?? 15_000);
  }

  async function restore(user: User | null) {
    if (disposed) return;
    const attempt = invalidate();
    if (!user) {
      options.onState({ phase: "signed-out" });
      return;
    }
    options.onState({ phase: "loading" });
    watch(attempt);
    try {
      const profile = await options.loadProfile(user);
      if (disposed || attempt !== generation) return;
      if (options.currentUser()?.uid !== user.uid) {
        void restore(options.currentUser());
        return;
      }
      if (profile.uid !== user.uid) throw new Error("profile mismatch");
      clearTimeout(timer);
      options.onState({ phase: "ready", profile });
    } catch {
      fail(attempt);
    }
  }

  options.onState({ phase: "loading" });
  watch(generation);
  try {
    unsubscribe = options.observe((user) => {
      // Firebase publishes a new user before signup finishes creating its profile.
      if (!authenticating) void restore(user);
    }, () => fail(generation));
  } catch {
    fail(generation);
  }

  return {
    async authenticate(action: () => Promise<unknown>) {
      if (disposed || authenticating) return;
      authenticating = true;
      invalidate();
      try {
        await action();
      } catch (error) {
        authenticating = false;
        // Preserve the login form for rejected credentials, but never assume
        // the auth provider is still signed out after a partially completed signup.
        void restore(options.currentUser());
        throw error;
      }
      authenticating = false;
      await restore(options.currentUser());
    },
    dispose() {
      disposed = true;
      invalidate();
      unsubscribe?.();
    },
  };
}
