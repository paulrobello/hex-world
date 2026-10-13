// Shared console-error matcher for the browser scripts. Bevy styles its level as "%cERROR%c", where \b does
// not hold, so match that marker explicitly. ERROR is case-sensitive: the known boid_material WARN contains
// a lowercase "error:".
export const BAD_TEXT = { test: (t) => /%cERROR%c|\bERROR\b/.test(t) || /uncaptured|invalid/i.test(t) };
