(ns my-project.utils-test
  (:require [clojure.test :refer [deftest is]]
            [my-project.utils :as utils]))

(deftest test-add
  (is (= 3 (utils/add 1 2))))