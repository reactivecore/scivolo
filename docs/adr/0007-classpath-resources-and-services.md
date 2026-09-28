# ADR 0007 — Resources, service providers and `Class.forName` from the classpath

Status: accepted · Date: 2026-09-28

## Context
Scala Native's linker resolves three reflective lookups at link time: it embeds resource files
(`embedResources`), lists service providers the build opts into (`serviceProviders`), and
builds the table `Class.forName` searches from the linked classes. The VM links lazily and
has no build configuration.

## Decision
- **Resources:** the `scalanative_resource_*` externs serve the files of the classpath's
  directories and jars (not the library image), with Scala Native's built-in exclusions
  (`META-INF/`, `scala-native/`, license and build files, classes, NIR, TASTy, sources); the
  first of equal paths wins. The list is made on first use, each content read on first use into
  memory the VM owns, outside the collected heap. Scala Native's `Class.getResourceAsStream`
  runs unchanged on top.
- **Service providers:** `ServiceLoader.load(service)` lists every class that the classpath's
  `META-INF/services/<service>` files name, as the JVM does. Each provider's `get` makes an
  instance with the constructor without parameters (a VM function passed to Scala Native's
  `createIntrinsicProvider`), or throws `ServiceConfigurationError` when the class is missing,
  abstract, not a subclass of the service or without such a constructor.
- **`Class.forName(name)`:** links and initializes the class, module or trait `name` on the
  classpath; otherwise `ClassNotFoundException`.

## Consequences
- Scripts get resources and service providers without configuration.
- Unlike Scala Native, a provider listed in `META-INF/services` cannot be left out: Scala
  Native's `ServiceLoaderTest.loadService` expects its third, not opted-in provider to be absent
  and fails.
- `Class.forName` finds classes a Scala Native link would have dropped as unreachable.
