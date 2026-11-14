scope custom<T: Resource>(namespace: String) { path "apis/{T::GROUP}/{T::VERSION}/namespaces/{namespace}/{T::PLURAL}"; GET list(..listing) -> List<T>; }
